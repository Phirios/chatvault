use anyhow::{anyhow, Context, Result};
use mail_parser::{MessageParser as MailParser, MimeHeaders};
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    config::{env_path, env_required},
    routes::import_file_for_chat_name,
    state::AppState,
};

#[derive(Clone)]
struct EmailConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
    fixed_delay: Duration,
    subject_prefixes: Vec<String>,
    scratch_dir: PathBuf,
}

struct PendingEmailImport {
    chat_name: String,
    file_name: String,
    path: PathBuf,
}

pub fn maybe_start_email_importer(state: AppState) -> Result<()> {
    if !env_bool("CHATVAULT_EMAIL_ENABLED", false) {
        return Ok(());
    }

    let config = EmailConfig::from_env()?;
    info!("starting optional IMAP email importer");

    tokio::spawn(async move {
        loop {
            let poll_config = config.clone();
            let poll_result =
                tokio::task::spawn_blocking(move || poll_email_once(&poll_config)).await;

            match poll_result {
                Ok(Ok(imports)) => {
                    for import in imports {
                        if let Err(error) = import_file_for_chat_name(
                            &state,
                            &import.chat_name,
                            &import.path,
                            &import.file_name,
                        )
                        .await
                        {
                            error!(
                                "email attachment import failed for {}: {:?}",
                                import.file_name, error
                            );
                        }
                        let _ = tokio::fs::remove_file(&import.path).await;
                    }
                }
                Ok(Err(error)) => error!("email polling failed: {error:?}"),
                Err(error) => error!("email polling task failed: {error:?}"),
            }

            sleep(config.fixed_delay).await;
        }
    });

    Ok(())
}

impl EmailConfig {
    fn from_env() -> Result<Self> {
        let fixed_delay_ms = env::var("CHATVAULT_EMAIL_FIXED_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10_000);
        let subject_prefixes = env::var("CHATVAULT_EMAIL_SUBJECT_STARTS_WITH")
            .unwrap_or_else(|_| "chat-vault,Conversa do WhatsApp com".to_string())
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();

        if subject_prefixes.is_empty() {
            return Err(anyhow!(
                "CHATVAULT_EMAIL_SUBJECT_STARTS_WITH cannot be empty"
            ));
        }

        Ok(Self {
            host: env_required("CHATVAULT_EMAIL_HOST")?,
            port: env::var("CHATVAULT_EMAIL_PORT")
                .unwrap_or_else(|_| "993".to_string())
                .parse()
                .context("CHATVAULT_EMAIL_PORT must be a valid port")?,
            username: env_required("CHATVAULT_EMAIL_USERNAME")?,
            password: env_required("CHATVAULT_EMAIL_PASSWORD")?,
            fixed_delay: Duration::from_millis(fixed_delay_ms),
            subject_prefixes,
            scratch_dir: env_path("CHATVAULT_SCRATCH_DIR", "/tmp/chatvault"),
        })
    }
}

fn poll_email_once(config: &EmailConfig) -> Result<Vec<PendingEmailImport>> {
    let client = imap::ClientBuilder::new(&config.host, config.port).connect()?;
    let mut session = client
        .login(&config.username, &config.password)
        .map_err(|(error, _client)| error)?;
    session.select("INBOX")?;

    let mut imports = Vec::new();
    let unseen = session.search("UNSEEN")?;
    for seq in unseen {
        let messages = session.fetch(seq.to_string(), "RFC822")?;
        let Some(message) = messages.iter().next() else {
            continue;
        };
        let Some(body) = message.body() else {
            warn!("email sequence {} did not contain RFC822 body", seq);
            continue;
        };

        match extract_imports_from_email(config, body) {
            Ok(mut found) => {
                imports.append(&mut found);
                session.store(seq.to_string(), "+FLAGS (\\Seen)")?;
            }
            Err(error) => warn!("failed to parse email sequence {}: {:?}", seq, error),
        }
    }

    session.logout()?;
    Ok(imports)
}

fn extract_imports_from_email(
    config: &EmailConfig,
    body: &[u8],
) -> Result<Vec<PendingEmailImport>> {
    let message = MailParser::default()
        .parse(body)
        .ok_or_else(|| anyhow!("failed to parse RFC822 message"))?;
    let subject = message.subject().unwrap_or_default();
    let Some(prefix) = config
        .subject_prefixes
        .iter()
        .find(|prefix| subject.to_lowercase().starts_with(&prefix.to_lowercase()))
    else {
        return Ok(Vec::new());
    };

    let chat_name = subject
        .get(prefix.len()..)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(subject)
        .to_string();

    let mut imports = Vec::new();
    for attachment in message.attachments() {
        let Some(file_name) = attachment.attachment_name().map(str::to_string) else {
            continue;
        };
        let path = scratch_path(&config.scratch_dir, &file_name);
        std::fs::write(&path, attachment.contents())?;
        imports.push(PendingEmailImport {
            chat_name: chat_name.clone(),
            file_name,
            path,
        });
    }

    Ok(imports)
}

fn scratch_path(scratch_dir: &Path, file_name: &str) -> PathBuf {
    let safe_name = file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    scratch_dir.join(format!("email-{}-{safe_name}", Uuid::new_v4()))
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
