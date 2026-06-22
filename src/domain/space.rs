use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::auth::AuthUser;
use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSpace {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub slug: String,
    pub name: String,
    pub owner_user_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSpaceMember {
    pub space_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpaceMembership {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub slug: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug)]
pub enum SpaceError {
    InvalidInput(&'static str),
    SlugAlreadyExists,
    NotFound,
    StoreUnavailable,
}

impl SpaceError {
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
            Self::SlugAlreadyExists => "space_slug_already_exists",
            Self::NotFound => "space_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::SlugAlreadyExists => "space slug already exists in this organization",
            Self::NotFound => "space was not found",
            Self::StoreUnavailable => "space store is unavailable",
        }
    }
}

#[async_trait::async_trait]
pub trait SpaceStore: Send + Sync {
    async fn create_space(
        &self,
        space: StoredSpace,
        owner_member: StoredSpaceMember,
    ) -> Result<(), SpaceError>;

    async fn list_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Vec<SpaceMembership>, SpaceError>;

    async fn get_for_user(
        &self,
        user_id: Uuid,
        space_id: Uuid,
    ) -> Result<Option<SpaceMembership>, SpaceError>;
}

#[derive(Clone)]
pub struct SpaceService {
    store: std::sync::Arc<dyn SpaceStore>,
}

impl SpaceService {
    pub fn new(store: std::sync::Arc<dyn SpaceStore>) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        owner: AuthUser,
        organization_id: Uuid,
        name: String,
    ) -> Result<SpaceMembership, SpaceError> {
        let name = normalize_name(name)?;
        let slug = slugify(&name)?;
        let space = StoredSpace {
            id: ids::new_uuid_v7(),
            organization_id,
            slug,
            name,
            owner_user_id: owner.id,
        };
        let owner_member = StoredSpaceMember {
            space_id: space.id,
            user_id: owner.id,
            role: "owner".to_owned(),
            status: "active".to_owned(),
        };

        self.store.create_space(space.clone(), owner_member).await?;

        Ok(SpaceMembership {
            id: space.id,
            organization_id: space.organization_id,
            slug: space.slug,
            name: space.name,
            role: "owner".to_owned(),
        })
    }

    pub async fn list_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Vec<SpaceMembership>, SpaceError> {
        self.store.list_for_user(user_id, organization_id).await
    }

    pub async fn get_for_user(
        &self,
        user_id: Uuid,
        space_id: Uuid,
    ) -> Result<SpaceMembership, SpaceError> {
        self.store
            .get_for_user(user_id, space_id)
            .await?
            .ok_or(SpaceError::NotFound)
    }
}

fn normalize_name(name: String) -> Result<String, SpaceError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if (2..=100).contains(&name.len()) {
        Ok(name)
    } else {
        Err(SpaceError::InvalidInput(
            "space name must be between 2 and 100 characters",
        ))
    }
}

fn slugify(name: &str) -> Result<String, SpaceError> {
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
        Err(SpaceError::InvalidInput(
            "space name must include letters or numbers",
        ))
    } else {
        Ok(slug)
    }
}
