use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::auth::AuthUser;
use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOrganization {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOrganizationMember {
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationMembership {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug)]
pub enum OrganizationError {
    InvalidInput(&'static str),
    SlugAlreadyExists,
    NotFound,
    StoreUnavailable,
}

impl OrganizationError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::SlugAlreadyExists => StatusCode::CONFLICT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::SlugAlreadyExists => "organization_slug_already_exists",
            Self::NotFound => "organization_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::SlugAlreadyExists => "organization slug already exists",
            Self::NotFound => "organization was not found",
            Self::StoreUnavailable => "organization store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait OrganizationStore: Send + Sync {
    async fn create_organization(
        &self,
        organization: StoredOrganization,
        owner_member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError>;

    async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationError>;

    async fn get_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Option<OrganizationMembership>, OrganizationError>;

    async fn add_member_if_missing(
        &self,
        member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError>;
}

#[derive(Clone)]
pub struct OrganizationService {
    store: std::sync::Arc<dyn OrganizationStore>,
}

impl OrganizationService {
    pub fn new(store: std::sync::Arc<dyn OrganizationStore>) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        owner: AuthUser,
        name: String,
    ) -> Result<OrganizationMembership, OrganizationError> {
        let name = normalize_name(name)?;
        let slug = slugify(&name)?;
        let organization = StoredOrganization {
            id: ids::new_uuid_v7(),
            slug,
            name,
        };
        let owner_member = StoredOrganizationMember {
            organization_id: organization.id,
            user_id: owner.id,
            role: "owner".to_owned(),
            status: "active".to_owned(),
        };

        self.store
            .create_organization(organization.clone(), owner_member)
            .await?;

        Ok(OrganizationMembership {
            id: organization.id,
            slug: organization.slug,
            name: organization.name,
            role: "owner".to_owned(),
        })
    }

    pub async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationError> {
        self.store.list_for_user(user_id).await
    }

    pub async fn get_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<OrganizationMembership, OrganizationError> {
        self.store
            .get_for_user(user_id, organization_id)
            .await?
            .ok_or(OrganizationError::NotFound)
    }

    pub async fn add_member_if_missing(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
        role: String,
    ) -> Result<(), OrganizationError> {
        self.store
            .add_member_if_missing(StoredOrganizationMember {
                organization_id,
                user_id,
                role: normalize_member_role(role)?,
                status: "active".to_owned(),
            })
            .await
    }
}

fn normalize_name(name: String) -> Result<String, OrganizationError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if (2..=100).contains(&name.len()) {
        Ok(name)
    } else {
        Err(OrganizationError::InvalidInput(
            "organization name must be between 2 and 100 characters",
        ))
    }
}

fn slugify(name: &str) -> Result<String, OrganizationError> {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        Err(OrganizationError::InvalidInput(
            "organization name must include letters or numbers",
        ))
    } else {
        Ok(slug)
    }
}

fn normalize_member_role(role: String) -> Result<String, OrganizationError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "owner" | "admin" | "member" | "guest" => Ok(role.trim().to_ascii_lowercase()),
        _ => Err(OrganizationError::InvalidInput(
            "organization member role must be owner, admin, member, or guest",
        )),
    }
}
