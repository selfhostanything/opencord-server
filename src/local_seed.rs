use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::domain::attachment::NewAttachment;
use crate::domain::auth::{AuthError, AuthResult, AuthUser};
use crate::domain::bot::{BotApplication, CreateBotApplicationInput, RotateBotTokenInput};
use crate::domain::channel::Channel;
use crate::domain::meeting::{MeetingBundle, NewMeetingAttendee, NewMeetingReminder};
use crate::domain::message::{CreateMessageInput, Message};
use crate::domain::webhook::IncomingWebhook;
use crate::state::AppState;

const DEFAULT_OWNER_EMAIL: &str = "owner@opencord.local";
const DEFAULT_OWNER_DISPLAY_NAME: &str = "OpenCord Owner";
const DEFAULT_OWNER_PASSWORD: &str = "correct horse battery staple";
const DEFAULT_ORGANIZATION_NAME: &str = "OpenCord Local Alpha";
const DEFAULT_SPACE_NAME: &str = "Local Alpha";
const DEFAULT_TEXT_CHANNEL_NAME: &str = "general";
const DEFAULT_VOICE_CHANNEL_NAME: &str = "Voice Lounge";
const DEFAULT_MEETING_TITLE: &str = "OpenCord Local Alpha Standup";
const DEFAULT_BOT_NAME: &str = "OpenCord Local Bot";
const DEFAULT_WEBHOOK_NAME: &str = "OpenCord Local Webhook";
const WELCOME_MESSAGE: &str = "Welcome to the OpenCord local alpha workspace.";
const RICH_MESSAGE: &str = "Local alpha rich message fixture.";
const ATTACHMENT_MESSAGE: &str = "Local alpha attachment fixture.";
const ATTACHMENT_FILE_NAME: &str = "local-alpha-readme.txt";
const ATTACHMENT_CONTENT_TYPE: &str = "text/plain";
const ATTACHMENT_CONTENT: &[u8] =
    b"OpenCord local alpha attachment fixture.\nUse this file to verify attachment rendering.\n";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalAlphaSeedOptions {
    pub owner_email: String,
    pub owner_display_name: String,
    pub owner_password: String,
    pub organization_name: String,
    pub space_name: String,
    pub text_channel_name: String,
    pub voice_channel_name: String,
    pub meeting_title: String,
    pub bot_name: String,
    pub webhook_name: String,
}

impl LocalAlphaSeedOptions {
    pub fn from_env() -> Self {
        Self {
            owner_email: env_or_default("OPENCORD_SEED_OWNER_EMAIL", DEFAULT_OWNER_EMAIL),
            owner_display_name: env_or_default(
                "OPENCORD_SEED_OWNER_DISPLAY_NAME",
                DEFAULT_OWNER_DISPLAY_NAME,
            ),
            owner_password: env_or_default("OPENCORD_SEED_OWNER_PASSWORD", DEFAULT_OWNER_PASSWORD),
            organization_name: env_or_default(
                "OPENCORD_SEED_ORGANIZATION_NAME",
                DEFAULT_ORGANIZATION_NAME,
            ),
            space_name: env_or_default("OPENCORD_SEED_SPACE_NAME", DEFAULT_SPACE_NAME),
            text_channel_name: env_or_default(
                "OPENCORD_SEED_TEXT_CHANNEL_NAME",
                DEFAULT_TEXT_CHANNEL_NAME,
            ),
            voice_channel_name: env_or_default(
                "OPENCORD_SEED_VOICE_CHANNEL_NAME",
                DEFAULT_VOICE_CHANNEL_NAME,
            ),
            meeting_title: env_or_default("OPENCORD_SEED_MEETING_TITLE", DEFAULT_MEETING_TITLE),
            bot_name: env_or_default("OPENCORD_SEED_BOT_NAME", DEFAULT_BOT_NAME),
            webhook_name: env_or_default("OPENCORD_SEED_WEBHOOK_NAME", DEFAULT_WEBHOOK_NAME),
        }
    }
}

impl Default for LocalAlphaSeedOptions {
    fn default() -> Self {
        Self {
            owner_email: DEFAULT_OWNER_EMAIL.to_owned(),
            owner_display_name: DEFAULT_OWNER_DISPLAY_NAME.to_owned(),
            owner_password: DEFAULT_OWNER_PASSWORD.to_owned(),
            organization_name: DEFAULT_ORGANIZATION_NAME.to_owned(),
            space_name: DEFAULT_SPACE_NAME.to_owned(),
            text_channel_name: DEFAULT_TEXT_CHANNEL_NAME.to_owned(),
            voice_channel_name: DEFAULT_VOICE_CHANNEL_NAME.to_owned(),
            meeting_title: DEFAULT_MEETING_TITLE.to_owned(),
            bot_name: DEFAULT_BOT_NAME.to_owned(),
            webhook_name: DEFAULT_WEBHOOK_NAME.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalAlphaSeedReport {
    pub owner: SeedOwner,
    pub organization: SeedOrganization,
    pub space: SeedSpace,
    pub channels: SeedChannels,
    pub messages: SeedMessages,
    pub meeting: SeedMeeting,
    pub bot: SeedBot,
    pub webhook: SeedWebhook,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedOwner {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub session_token: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedOrganization {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedSpace {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedChannels {
    pub text: SeedChannel,
    pub voice: SeedChannel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedChannel {
    pub id: Uuid,
    pub kind: String,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedMessages {
    pub welcome: SeedMessage,
    pub rich: SeedMessage,
    pub attachment_fixture: SeedMessage,
    pub attachment: SeedAttachment,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedMessage {
    pub id: Uuid,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedAttachment {
    pub id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedMeeting {
    pub id: Uuid,
    pub title: String,
    pub join_slug: String,
    pub join_url: String,
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedBot {
    pub application_id: Uuid,
    pub bot_user_id: Uuid,
    pub name: String,
    pub token: String,
    pub token_last_four: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeedWebhook {
    pub id: Uuid,
    pub bot_user_id: Uuid,
    pub name: String,
    pub token: String,
    pub token_last_four: String,
    pub execute_url: String,
}

pub async fn seed_local_alpha(
    state: &AppState,
    options: LocalAlphaSeedOptions,
) -> Result<LocalAlphaSeedReport> {
    let auth = authenticate_owner(state, &options).await?;
    let organization = ensure_organization(state, &auth.user, &options).await?;
    let space = ensure_space(state, &auth.user, organization.id, &options).await?;
    let text_channel = ensure_channel(
        state,
        organization.id,
        space.id,
        "text",
        &options.text_channel_name,
        Some("Local alpha chat, bot, webhook, and attachment smoke tests."),
    )
    .await?;
    let voice_channel = ensure_channel(
        state,
        organization.id,
        space.id,
        "voice",
        &options.voice_channel_name,
        Some("Local alpha voice and screen-share smoke tests."),
    )
    .await?;
    let messages = ensure_messages(
        state,
        organization.id,
        space.id,
        text_channel.id,
        auth.user.id,
    )
    .await?;
    let meeting = ensure_meeting(
        state,
        organization.id,
        space.id,
        text_channel.id,
        auth.user.id,
        &auth.user.email,
        &options,
    )
    .await?;
    let bot = ensure_bot(state, organization.id, space.id, auth.user.id, &options).await?;
    let webhook = ensure_webhook(
        state,
        organization.id,
        space.id,
        text_channel.id,
        auth.user.id,
        &options,
    )
    .await?;

    Ok(LocalAlphaSeedReport {
        owner: SeedOwner {
            user_id: auth.user.id,
            email: auth.user.email,
            display_name: auth.user.display_name,
            session_token: auth.session_token,
        },
        organization: SeedOrganization {
            id: organization.id,
            slug: organization.slug,
            name: organization.name,
        },
        space: SeedSpace {
            id: space.id,
            slug: space.slug,
            name: space.name,
        },
        channels: SeedChannels {
            text: seed_channel(text_channel),
            voice: seed_channel(voice_channel),
        },
        messages,
        meeting: SeedMeeting {
            id: meeting.meeting.id,
            title: meeting.meeting.title,
            join_slug: meeting.meeting.join_slug.clone(),
            join_url: join_url(&state.config.public_url, &meeting.meeting.join_slug),
            starts_at: meeting.meeting.starts_at,
            ends_at: meeting.meeting.ends_at,
        },
        bot,
        webhook: SeedWebhook {
            id: webhook.webhook.id,
            bot_user_id: webhook.webhook.bot_user_id,
            name: webhook.webhook.name,
            token_last_four: webhook.webhook.token_last_four,
            execute_url: webhook_url(&state.config.public_url, webhook.webhook.id, &webhook.token),
            token: webhook.token,
        },
    })
}

async fn authenticate_owner(
    state: &AppState,
    options: &LocalAlphaSeedOptions,
) -> Result<AuthResult> {
    match state
        .auth
        .register(
            options.owner_email.clone(),
            options.owner_display_name.clone(),
            options.owner_password.clone(),
        )
        .await
    {
        Ok(auth) => Ok(auth),
        Err(AuthError::EmailAlreadyRegistered) => state
            .auth
            .login(options.owner_email.clone(), options.owner_password.clone())
            .await
            .map_err(|error| {
                anyhow!(
                    "seed owner already exists but login failed: {}",
                    error.message()
                )
            }),
        Err(error) => Err(anyhow!("create seed owner: {}", error.message())),
    }
}

async fn ensure_organization(
    state: &AppState,
    owner: &AuthUser,
    options: &LocalAlphaSeedOptions,
) -> Result<crate::domain::organization::OrganizationMembership> {
    let existing = state
        .organizations
        .list_for_user(owner.id)
        .await
        .map_err(|error| anyhow!("list seed organizations: {}", error.message()))?
        .into_iter()
        .find(|organization| organization.name == options.organization_name);
    if let Some(organization) = existing {
        return Ok(organization);
    }

    state
        .organizations
        .create(owner.clone(), options.organization_name.clone())
        .await
        .map_err(|error| anyhow!("create seed organization: {}", error.message()))
}

async fn ensure_space(
    state: &AppState,
    owner: &AuthUser,
    organization_id: Uuid,
    options: &LocalAlphaSeedOptions,
) -> Result<crate::domain::space::SpaceMembership> {
    let existing = state
        .spaces
        .list_for_user(owner.id, organization_id)
        .await
        .map_err(|error| anyhow!("list seed spaces: {}", error.message()))?
        .into_iter()
        .find(|space| space.name == options.space_name);
    if let Some(space) = existing {
        return Ok(space);
    }

    state
        .spaces
        .create(owner.clone(), organization_id, options.space_name.clone())
        .await
        .map_err(|error| anyhow!("create seed space: {}", error.message()))
}

async fn ensure_channel(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    kind: &str,
    name: &str,
    topic: Option<&str>,
) -> Result<Channel> {
    let existing = state
        .channels
        .list_for_space(space_id)
        .await
        .map_err(|error| anyhow!("list seed channels: {}", error.message()))?
        .into_iter()
        .find(|channel| channel.kind == kind && channel.name == name);
    if let Some(channel) = existing {
        return Ok(channel);
    }

    state
        .channels
        .create(
            organization_id,
            space_id,
            Some(kind.to_owned()),
            name.to_owned(),
            topic.map(ToOwned::to_owned),
            false,
        )
        .await
        .map_err(|error| anyhow!("create seed {kind} channel: {}", error.message()))
}

async fn ensure_messages(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
) -> Result<SeedMessages> {
    let welcome =
        ensure_welcome_message(state, organization_id, space_id, channel_id, owner_user_id).await?;
    let rich = ensure_rich_message(
        state,
        organization_id,
        space_id,
        channel_id,
        owner_user_id,
        welcome.id,
    )
    .await?;
    let (attachment_fixture, attachment) =
        ensure_attachment_message(state, organization_id, space_id, channel_id, owner_user_id)
            .await?;

    Ok(SeedMessages {
        welcome: seed_message(welcome),
        rich: seed_message(rich),
        attachment_fixture: seed_message(attachment_fixture),
        attachment,
    })
}

async fn ensure_welcome_message(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Message> {
    if let Some(message) = message_by_content(state, channel_id, WELCOME_MESSAGE).await? {
        return Ok(message);
    }

    state
        .messages
        .create(
            organization_id,
            Some(space_id),
            channel_id,
            owner_user_id,
            WELCOME_MESSAGE.to_owned(),
            false,
        )
        .await
        .map_err(|error| anyhow!("create seed welcome message: {}", error.message()))
}

async fn ensure_rich_message(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
    reply_to_message_id: Uuid,
) -> Result<Message> {
    if let Some(message) = message_by_content(state, channel_id, RICH_MESSAGE).await? {
        return Ok(message);
    }

    state
        .messages
        .create_with_embeds(CreateMessageInput {
            organization_id,
            space_id: Some(space_id),
            channel_id,
            author_user_id: owner_user_id,
            content: RICH_MESSAGE.to_owned(),
            allow_empty_content: false,
            embeds: vec![json!({
                "title": "OpenCord Local Alpha",
                "description": "Fixture for rich message rendering.",
                "color": 3_726_513_u64,
                "fields": [
                    {
                        "name": "Surface",
                        "value": "embeds, mentions, replies, and components",
                        "inline": true
                    }
                ],
                "footer": {
                    "text": "Phase 09"
                }
            })],
            components: vec![json!({
                "type": 1,
                "components": [
                    {
                        "type": 2,
                        "style": 1,
                        "label": "Acknowledge",
                        "custom_id": "local-alpha:ack"
                    }
                ]
            })],
            webhook_username: None,
            webhook_avatar_url: None,
            mention_user_ids: vec![owner_user_id],
            mention_role_ids: Vec::new(),
            mention_everyone: true,
            reply_to_message_id: Some(reply_to_message_id),
        })
        .await
        .map_err(|error| anyhow!("create seed rich message: {}", error.message()))
}

async fn ensure_attachment_message(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
) -> Result<(Message, SeedAttachment)> {
    let message =
        if let Some(message) = message_by_content(state, channel_id, ATTACHMENT_MESSAGE).await? {
            message
        } else {
            state
                .messages
                .create(
                    organization_id,
                    Some(space_id),
                    channel_id,
                    owner_user_id,
                    ATTACHMENT_MESSAGE.to_owned(),
                    false,
                )
                .await
                .map_err(|error| anyhow!("create seed attachment message: {}", error.message()))?
        };
    let attachments = state
        .attachments
        .list_for_message_ids(&[message.id])
        .await
        .map_err(|error| anyhow!("list seed attachment: {}", error.message()))?;
    if let Some(attachment) = attachments
        .into_iter()
        .find(|attachment| attachment.file_name == ATTACHMENT_FILE_NAME)
    {
        return Ok((message, seed_attachment(attachment)));
    }

    let pending = state
        .attachments
        .create_pending(NewAttachment {
            organization_id,
            space_id,
            channel_id,
            uploader_user_id: owner_user_id,
            file_name: ATTACHMENT_FILE_NAME.to_owned(),
            content_type: ATTACHMENT_CONTENT_TYPE.to_owned(),
            size_bytes: ATTACHMENT_CONTENT.len() as i64,
        })
        .await
        .map_err(|error| anyhow!("create seed attachment: {}", error.message()))?;
    let uploaded = state
        .attachments
        .upload(
            pending,
            owner_user_id,
            ATTACHMENT_CONTENT_TYPE.to_owned(),
            ATTACHMENT_CONTENT.to_vec(),
        )
        .await
        .map_err(|error| anyhow!("upload seed attachment: {}", error.message()))?;
    state
        .attachments
        .validate_for_message(
            organization_id,
            space_id,
            channel_id,
            owner_user_id,
            &[uploaded.id],
        )
        .await
        .map_err(|error| anyhow!("validate seed attachment: {}", error.message()))?;
    let linked = state
        .attachments
        .link_to_message(message.id, &[uploaded.id])
        .await
        .map_err(|error| anyhow!("link seed attachment: {}", error.message()))?
        .into_iter()
        .next()
        .context("linked seed attachment should be returned")?;

    Ok((message, seed_attachment(linked)))
}

async fn ensure_meeting(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
    owner_email: &str,
    options: &LocalAlphaSeedOptions,
) -> Result<MeetingBundle> {
    let existing = state
        .meetings
        .list_for_organization(organization_id)
        .await
        .map_err(|error| anyhow!("list seed meetings: {}", error.message()))?
        .into_iter()
        .find(|meeting| meeting.meeting.title == options.meeting_title);
    if let Some(meeting) = existing {
        return Ok(meeting);
    }

    state
        .meetings
        .create(
            organization_id,
            Some(space_id),
            Some(channel_id),
            owner_user_id,
            options.meeting_title.clone(),
            Some("Local alpha meeting fixture for calendar and reminder smoke tests.".to_owned()),
            "2099-01-09T09:00:00Z".to_owned(),
            "2099-01-09T09:30:00Z".to_owned(),
            Some("UTC".to_owned()),
            vec![NewMeetingAttendee {
                user_id: Some(owner_user_id),
                email: None,
                display_name: Some("OpenCord Owner".to_owned()),
                role: Some("host".to_owned()),
            }],
            vec![
                NewMeetingReminder {
                    recipient_user_id: Some(owner_user_id),
                    recipient_email: None,
                    channel: "in_app".to_owned(),
                    offset_minutes: 10,
                },
                NewMeetingReminder {
                    recipient_user_id: None,
                    recipient_email: Some(owner_email.to_owned()),
                    channel: "email".to_owned(),
                    offset_minutes: 15,
                },
            ],
        )
        .await
        .map_err(|error| anyhow!("create seed meeting: {}", error.message()))
}

async fn ensure_bot(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    owner_user_id: Uuid,
    options: &LocalAlphaSeedOptions,
) -> Result<SeedBot> {
    let (application, token, token_last_four) = match state
        .bots
        .list_applications_for_organization(organization_id)
        .await
        .map_err(|error| anyhow!("list seed bots: {}", error.message()))?
        .into_iter()
        .find(|application| application.name == options.bot_name)
    {
        Some(application) => {
            let token = state
                .bots
                .rotate_token(RotateBotTokenInput {
                    organization_id,
                    application_id: application.id,
                    created_by_user_id: owner_user_id,
                })
                .await
                .map_err(|error| anyhow!("rotate seed bot token: {}", error.message()))?;
            (application, token.token, token.token_last_four)
        }
        None => {
            let created = state
                .bots
                .create_application(CreateBotApplicationInput {
                    organization_id,
                    created_by_user_id: owner_user_id,
                    name: options.bot_name.clone(),
                    description: Some(
                        "Local alpha bot fixture for Discord-compatible API smoke tests."
                            .to_owned(),
                    ),
                })
                .await
                .map_err(|error| anyhow!("create seed bot: {}", error.message()))?;
            (
                created.application,
                created.token.token,
                created.token.token_last_four,
            )
        }
    };

    invite_bot_to_space(
        state,
        organization_id,
        space_id,
        owner_user_id,
        &application,
    )
    .await?;

    Ok(SeedBot {
        application_id: application.id,
        bot_user_id: application.bot_user_id,
        name: application.name,
        token,
        token_last_four,
    })
}

async fn invite_bot_to_space(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    owner_user_id: Uuid,
    application: &BotApplication,
) -> Result<()> {
    state
        .organizations
        .add_member_if_missing(
            organization_id,
            application.bot_user_id,
            "member".to_owned(),
        )
        .await
        .map_err(|error| anyhow!("add seed bot organization member: {}", error.message()))?;
    state
        .spaces
        .add_member(space_id, application.bot_user_id, "member".to_owned())
        .await
        .map_err(|error| anyhow!("add seed bot space member: {}", error.message()))?;
    state
        .permissions
        .require_space(
            owner_user_id,
            &state
                .spaces
                .get_for_user(owner_user_id, space_id)
                .await
                .map_err(|error| anyhow!("load owner seed space: {}", error.message()))?,
            crate::domain::permission::Permission::ManageSpace,
        )
        .await
        .map_err(|error| anyhow!("verify seed owner can manage space: {}", error.message()))?;
    Ok(())
}

struct SeedWebhookWithToken {
    webhook: IncomingWebhook,
    token: String,
}

async fn ensure_webhook(
    state: &AppState,
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    owner_user_id: Uuid,
    options: &LocalAlphaSeedOptions,
) -> Result<SeedWebhookWithToken> {
    match state
        .webhooks
        .list_for_channel(channel_id)
        .await
        .map_err(|error| anyhow!("list seed webhooks: {}", error.message()))?
        .into_iter()
        .find(|webhook| webhook.name == options.webhook_name)
    {
        Some(webhook) => {
            let rotated = state
                .webhooks
                .rotate_token(webhook.id, channel_id)
                .await
                .map_err(|error| anyhow!("rotate seed webhook token: {}", error.message()))?;
            Ok(SeedWebhookWithToken {
                webhook: rotated.webhook,
                token: rotated.token,
            })
        }
        None => {
            let created = state
                .webhooks
                .create(
                    organization_id,
                    space_id,
                    channel_id,
                    owner_user_id,
                    options.webhook_name.clone(),
                )
                .await
                .map_err(|error| anyhow!("create seed webhook: {}", error.message()))?;
            Ok(SeedWebhookWithToken {
                webhook: created.webhook,
                token: created.token,
            })
        }
    }
}

async fn message_by_content(
    state: &AppState,
    channel_id: Uuid,
    content: &str,
) -> Result<Option<Message>> {
    Ok(state
        .messages
        .list_for_channel(channel_id)
        .await
        .map_err(|error| anyhow!("list seed messages: {}", error.message()))?
        .into_iter()
        .find(|message| message.content == content))
}

fn seed_channel(channel: Channel) -> SeedChannel {
    SeedChannel {
        id: channel.id,
        kind: channel.kind,
        slug: channel.slug,
        name: channel.name,
    }
}

fn seed_message(message: Message) -> SeedMessage {
    SeedMessage {
        id: message.id,
        content: message.content,
    }
}

fn seed_attachment(attachment: crate::domain::attachment::Attachment) -> SeedAttachment {
    SeedAttachment {
        id: attachment.id,
        file_name: attachment.file_name,
        content_type: attachment.content_type,
        size_bytes: attachment.size_bytes,
    }
}

fn join_url(public_url: &str, join_slug: &str) -> String {
    format!("{}/join/{join_slug}", public_url.trim_end_matches('/'))
}

fn webhook_url(public_url: &str, webhook_id: Uuid, token: &str) -> String {
    format!(
        "{}/api/webhooks/{webhook_id}/{token}",
        public_url.trim_end_matches('/')
    )
}

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}
