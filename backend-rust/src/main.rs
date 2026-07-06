mod config;
mod db;
mod email;
mod error;
mod models;
mod parser;
mod response;
mod routes;
mod state;
mod storage;

use anyhow::{Context, Result};
use axum::{extract::DefaultBodyLimit, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::info;

use crate::{
    config::{env_parse, env_required, load_config},
    db::migrate,
    email::maybe_start_email_importer,
    routes::api_router,
    state::AppState,
    storage::build_storage,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = load_config()?;
    tokio::fs::create_dir_all(&config.scratch_dir).await?;

    let db = PgPoolOptions::new()
        .max_connections(env_parse("DATABASE_MAX_CONNECTIONS", 8)?)
        .connect(&env_required("DATABASE_URL")?)
        .await
        .context("failed to connect to postgres")?;

    migrate(&db).await?;

    let state = AppState {
        db,
        storage: build_storage()?,
        config: config.clone(),
    };
    maybe_start_email_importer(state.clone())?;

    let app = Router::new()
        .merge(api_router(state))
        .fallback_service(ServeDir::new(config.public_dir).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = config.bind.parse()?;
    info!("starting chatvault rust backend on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
