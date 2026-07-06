use anyhow::{Context, Result};
use std::{env, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub bind: String,
    pub public_dir: PathBuf,
    pub scratch_dir: PathBuf,
    pub version: String,
    pub msg_date_format: Option<String>,
}

pub fn load_config() -> Result<Config> {
    Ok(Config {
        bind: env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        public_dir: env_path("CHATVAULT_PUBLIC_DIR", "/app/public"),
        scratch_dir: env_path("CHATVAULT_SCRATCH_DIR", "/tmp/chatvault"),
        version: env::var("CHATVAULT_VERSION").unwrap_or_else(|_| "rust-0.1.0".to_string()),
        msg_date_format: env::var("CHATVAULT_MSGPARSER_DATEFORMAT")
            .ok()
            .filter(|s| !s.trim().is_empty()),
    })
}

pub fn env_required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}

pub fn env_path(name: &str, default: &str) -> PathBuf {
    env::var(name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

pub fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    env::var(name)
        .map(|value| Ok(value.parse()?))
        .unwrap_or(Ok(default))
}
