use sea_orm_migration::prelude::*;

mod m20260622173149_baseline;
mod m20260623012400_auth;
mod m20260623013800_organizations;
mod m20260623015000_spaces;
mod m20260623020400_channels;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260622173149_baseline::Migration),
            Box::new(m20260623012400_auth::Migration),
            Box::new(m20260623013800_organizations::Migration),
            Box::new(m20260623015000_spaces::Migration),
            Box::new(m20260623020400_channels::Migration),
        ]
    }
}
