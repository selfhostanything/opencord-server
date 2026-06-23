use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::controllers::message_controller::{attachment_download_url, attachment_response};
use crate::domain::attachment::{AttachmentError, NewAttachment};
use crate::domain::auth::AuthError;
use crate::domain::channel::ChannelError;
use crate::domain::permission::{Permission, PermissionError};
use crate::domain::rate_limit::{
    RateLimitDecision, attachment_presign_bucket, attachment_upload_bucket,
};
use crate::domain::space::SpaceError;
use crate::http::rate_limit::rate_limit_headers;
use crate::http::session::bearer_token;
use crate::models::attachment::{
    AttachmentPresignResponse, AttachmentResourceResponse, AttachmentUploadResponse,
    PresignAttachmentRequest,
};
use crate::models::auth::{ErrorDetail, ErrorResponse};
use crate::state::AppState;

pub async fn presign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PresignAttachmentRequest>,
) -> Result<impl IntoResponse, AttachmentApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let channel = state.channels.get(request.channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::SendMessages)
        .await?;
    let rate_limit = state
        .attachment_presign_rate_limits
        .check(attachment_presign_bucket(user.id, channel.id));
    if !rate_limit.allowed {
        return Err(AttachmentApiError::RateLimited(rate_limit));
    }

    let attachment = state
        .attachments
        .create_pending(NewAttachment {
            organization_id: channel.organization_id,
            space_id: channel.space_id,
            channel_id: channel.id,
            uploader_user_id: user.id,
            file_name: request.file_name,
            content_type: request.content_type,
            size_bytes: request.size_bytes,
        })
        .await?;
    let upload_url = attachment_download_url(&state.config.public_url, attachment.id);

    Ok((
        StatusCode::CREATED,
        rate_limit_headers(&rate_limit),
        Json(AttachmentPresignResponse {
            attachment: attachment_response(attachment, &state.config.public_url),
            upload: AttachmentUploadResponse {
                method: "PUT",
                url: upload_url,
            },
        }),
    ))
}

pub async fn upload_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attachment_id): Path<Uuid>,
    body: Bytes,
) -> Result<impl IntoResponse, AttachmentApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let attachment = state.attachments.get(attachment_id).await?;
    let channel = state.channels.get(attachment.channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::SendMessages)
        .await?;
    let rate_limit = state
        .attachment_upload_rate_limits
        .check(attachment_upload_bucket(user.id));
    if !rate_limit.allowed {
        return Err(AttachmentApiError::RateLimited(rate_limit));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let attachment = state
        .attachments
        .upload(attachment, user.id, content_type, body.to_vec())
        .await?;

    Ok((
        rate_limit_headers(&rate_limit),
        Json(AttachmentResourceResponse {
            attachment: attachment_response(attachment, &state.config.public_url),
        }),
    ))
}

pub async fn download_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attachment_id): Path<Uuid>,
) -> Result<Response, AttachmentApiError> {
    let token = bearer_token(&headers)?;
    let user = state.auth.user_for_token(token).await?;
    let attachment = state.attachments.get(attachment_id).await?;
    let channel = state.channels.get(attachment.channel_id).await?;
    let space = state.spaces.get_for_user(user.id, channel.space_id).await?;
    state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await?;

    let content = state.attachments.content(attachment_id).await?;
    let mut response = Response::new(Body::from(content.bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    Ok(response)
}

#[derive(Debug)]
pub enum AttachmentApiError {
    Auth(AuthError),
    Channel(ChannelError),
    Space(SpaceError),
    Permission(PermissionError),
    Attachment(AttachmentError),
    RateLimited(RateLimitDecision),
}

impl From<AuthError> for AttachmentApiError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<ChannelError> for AttachmentApiError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

impl From<SpaceError> for AttachmentApiError {
    fn from(error: SpaceError) -> Self {
        Self::Space(error)
    }
}

impl From<PermissionError> for AttachmentApiError {
    fn from(error: PermissionError) -> Self {
        Self::Permission(error)
    }
}

impl From<AttachmentError> for AttachmentApiError {
    fn from(error: AttachmentError) -> Self {
        Self::Attachment(error)
    }
}

impl IntoResponse for AttachmentApiError {
    fn into_response(self) -> Response {
        if let Self::RateLimited(decision) = self {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                rate_limit_headers(&decision),
                Json(ErrorResponse {
                    error: ErrorDetail {
                        code: "rate_limited",
                        message: "rate limit exceeded",
                    },
                }),
            )
                .into_response();
        }

        let (status, code, message) = match self {
            Self::Auth(error) => (error.status_code(), error.code(), error.message()),
            Self::Channel(error) => (error.status_code(), error.code(), error.message()),
            Self::Space(error) => (error.status_code(), error.code(), error.message()),
            Self::Permission(error) => (error.status_code(), error.code(), error.message()),
            Self::Attachment(error) => (error.status_code(), error.code(), error.message()),
            Self::RateLimited(_) => unreachable!("rate limited responses are returned above"),
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorDetail { code, message },
            }),
        )
            .into_response()
    }
}
