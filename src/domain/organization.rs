use axum::http::StatusCode;
use uuid::Uuid;

use crate::domain::auth::AuthUser;
use crate::domain::ids;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOrganization {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredOrganizationMember {
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredCustomDomain {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub hostname: String,
    pub verification_token: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomDomain {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub hostname: String,
    pub verification_token: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomDomainTenant {
    pub organization_id: Uuid,
    pub slug: String,
    pub name: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
    pub hostname: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationMembership {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub role: String,
    pub plan: String,
    pub deployment_mode: String,
    pub primary_region: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantProvision {
    pub organization: OrganizationMembership,
    pub owner_user_id: Uuid,
}

#[derive(Debug)]
pub enum OrganizationError {
    InvalidInput(&'static str),
    SlugAlreadyExists,
    CustomDomainAlreadyExists,
    Forbidden,
    NotFound,
    StoreUnavailable,
}

impl OrganizationError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::SlugAlreadyExists => StatusCode::CONFLICT,
            Self::CustomDomainAlreadyExists => StatusCode::CONFLICT,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StoreUnavailable => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::SlugAlreadyExists => "organization_slug_already_exists",
            Self::CustomDomainAlreadyExists => "custom_domain_already_exists",
            Self::Forbidden => "forbidden",
            Self::NotFound => "organization_not_found",
            Self::StoreUnavailable => "store_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::SlugAlreadyExists => "organization slug already exists",
            Self::CustomDomainAlreadyExists => "custom domain already exists",
            Self::Forbidden => "organization admin permission is required",
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

    async fn active_member_user_ids(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Uuid>, OrganizationError>;

    async fn update_plan(
        &self,
        organization_id: Uuid,
        plan: String,
    ) -> Result<(), OrganizationError>;

    async fn add_member_if_missing(
        &self,
        member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError>;

    async fn create_custom_domain(
        &self,
        custom_domain: StoredCustomDomain,
    ) -> Result<(), OrganizationError>;

    async fn list_custom_domains(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<CustomDomain>, OrganizationError>;

    async fn verify_custom_domain(
        &self,
        organization_id: Uuid,
        custom_domain_id: Uuid,
        verification_token: String,
    ) -> Result<CustomDomain, OrganizationError>;

    async fn resolve_custom_domain(
        &self,
        hostname: String,
    ) -> Result<Option<CustomDomainTenant>, OrganizationError>;
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
        self.create_with_options(owner, name, "free", "self_hosted", "local")
            .await
    }

    pub async fn provision_tenant(
        &self,
        owner: AuthUser,
        name: String,
        plan: String,
        deployment_mode: String,
        primary_region: String,
    ) -> Result<TenantProvision, OrganizationError> {
        let owner_user_id = owner.id;
        let plan = normalize_plan(plan)?;
        let deployment_mode = normalize_deployment_mode(deployment_mode)?;
        let primary_region = normalize_region(primary_region)?;
        let organization = self
            .create_with_options(owner, name, &plan, &deployment_mode, &primary_region)
            .await?;

        Ok(TenantProvision {
            organization,
            owner_user_id,
        })
    }

    async fn create_with_options(
        &self,
        owner: AuthUser,
        name: String,
        plan: &str,
        deployment_mode: &str,
        primary_region: &str,
    ) -> Result<OrganizationMembership, OrganizationError> {
        let name = normalize_name(name)?;
        let slug = slugify(&name)?;
        let organization = StoredOrganization {
            id: ids::new_uuid_v7(),
            slug,
            name,
            plan: plan.to_owned(),
            deployment_mode: deployment_mode.to_owned(),
            primary_region: primary_region.to_owned(),
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
            plan: organization.plan,
            deployment_mode: organization.deployment_mode,
            primary_region: organization.primary_region,
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

    pub async fn require_admin(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<OrganizationMembership, OrganizationError> {
        let membership = self.get_for_user(user_id, organization_id).await?;
        if membership.role == "owner" || membership.role == "admin" {
            Ok(membership)
        } else {
            Err(OrganizationError::Forbidden)
        }
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

    pub async fn create_custom_domain(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        hostname: String,
    ) -> Result<CustomDomain, OrganizationError> {
        self.get_for_user(user_id, organization_id).await?;

        let hostname = normalize_hostname(hostname)?;
        let custom_domain = StoredCustomDomain {
            id: ids::new_uuid_v7(),
            organization_id,
            hostname,
            verification_token: format!("opc-domain-{}", ids::new_uuid_v7().simple()),
            status: "pending_verification".to_owned(),
        };

        self.store
            .create_custom_domain(custom_domain.clone())
            .await?;

        Ok(CustomDomain::from(custom_domain))
    }

    pub async fn list_custom_domains(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Vec<CustomDomain>, OrganizationError> {
        self.get_for_user(user_id, organization_id).await?;
        self.store.list_custom_domains(organization_id).await
    }

    pub async fn verify_custom_domain(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        custom_domain_id: Uuid,
        verification_token: String,
    ) -> Result<CustomDomain, OrganizationError> {
        self.get_for_user(user_id, organization_id).await?;

        let verification_token = verification_token.trim();
        if verification_token.is_empty() {
            return Err(OrganizationError::InvalidInput(
                "custom domain verification_token is required",
            ));
        }

        self.store
            .verify_custom_domain(
                organization_id,
                custom_domain_id,
                verification_token.to_owned(),
            )
            .await
    }

    pub async fn resolve_custom_domain(
        &self,
        host_header: String,
    ) -> Result<CustomDomainTenant, OrganizationError> {
        let hostname = normalize_host_header(host_header)?;
        self.store
            .resolve_custom_domain(hostname)
            .await?
            .ok_or(OrganizationError::NotFound)
    }
}

impl From<StoredCustomDomain> for CustomDomain {
    fn from(custom_domain: StoredCustomDomain) -> Self {
        Self {
            id: custom_domain.id,
            organization_id: custom_domain.organization_id,
            hostname: custom_domain.hostname,
            verification_token: custom_domain.verification_token,
            status: custom_domain.status,
        }
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

fn normalize_plan(plan: String) -> Result<String, OrganizationError> {
    match plan.trim().to_ascii_lowercase().as_str() {
        "free" | "team" | "business" | "enterprise" => Ok(plan.trim().to_ascii_lowercase()),
        _ => Err(OrganizationError::InvalidInput(
            "tenant plan must be free, team, business, or enterprise",
        )),
    }
}

fn normalize_deployment_mode(deployment_mode: String) -> Result<String, OrganizationError> {
    match deployment_mode.trim().to_ascii_lowercase().as_str() {
        "self_hosted" | "cloud" => Ok(deployment_mode.trim().to_ascii_lowercase()),
        _ => Err(OrganizationError::InvalidInput(
            "tenant deployment_mode must be self_hosted or cloud",
        )),
    }
}

fn normalize_region(region: String) -> Result<String, OrganizationError> {
    let region = region.trim().to_ascii_lowercase();
    if (2..=64).contains(&region.len())
        && region
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        Ok(region)
    } else {
        Err(OrganizationError::InvalidInput(
            "tenant primary_region must be 2 to 64 lowercase letters, numbers, or hyphens",
        ))
    }
}

fn normalize_host_header(host_header: String) -> Result<String, OrganizationError> {
    let host = host_header.trim();
    if host.is_empty() {
        return Err(OrganizationError::InvalidInput("host header is required"));
    }

    if let Some(host_without_port) = host
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
    {
        return normalize_hostname(host_without_port.to_owned());
    }

    let hostname = host
        .rsplit_once(':')
        .filter(|(_, port)| {
            !port.is_empty() && port.chars().all(|character| character.is_ascii_digit())
        })
        .map(|(hostname, _)| hostname)
        .unwrap_or(host);

    normalize_hostname(hostname.to_owned())
}

fn normalize_hostname(hostname: String) -> Result<String, OrganizationError> {
    let hostname = hostname.trim().trim_end_matches('.').to_ascii_lowercase();

    if hostname.len() > 253 || !hostname.contains('.') {
        return Err(OrganizationError::InvalidInput(
            "custom domain hostname must be a valid fully qualified hostname",
        ));
    }

    let valid = hostname.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    });

    if valid {
        Ok(hostname)
    } else {
        Err(OrganizationError::InvalidInput(
            "custom domain hostname must be a valid fully qualified hostname",
        ))
    }
}
