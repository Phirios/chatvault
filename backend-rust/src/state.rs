use crate::{config::Config, storage::Storage};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub storage: Arc<dyn Storage>,
    pub config: Config,
}
