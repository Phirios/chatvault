use anyhow::{anyhow, Result};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

use crate::models::{ChatLastMessageOutput, MessageOutput};

pub async fn migrate(db: &PgPool) -> Result<()> {
    let statements = [
        r#"
        CREATE TABLE IF NOT EXISTS chat (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            external_id TEXT UNIQUE,
            bucket TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS message (
            id BIGSERIAL PRIMARY KEY,
            author TEXT NOT NULL,
            author_type TEXT NOT NULL,
            created_at TIMESTAMP NOT NULL,
            content TEXT NOT NULL,
            attachment_path TEXT,
            attachment_name TEXT,
            chat_id BIGINT NOT NULL REFERENCES chat(id) ON DELETE CASCADE,
            external_id TEXT UNIQUE
        )
        "#,
        "CREATE INDEX IF NOT EXISTS idx_message_chat_id_id ON message(chat_id, id DESC)",
        "CREATE INDEX IF NOT EXISTS idx_message_chat_id_created_at ON message(chat_id, created_at, id)",
    ];

    for statement in statements {
        sqlx::query(statement).execute(db).await?;
    }

    Ok(())
}

pub async fn find_or_create_chat(db: &PgPool, name: &str) -> Result<i64> {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM chat WHERE name = $1")
        .bind(name)
        .fetch_optional(db)
        .await?
    {
        return Ok(id);
    }
    let bucket = format!("{}/", Uuid::new_v4());
    Ok(
        sqlx::query_scalar("INSERT INTO chat(name, bucket) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(bucket)
            .fetch_one(db)
            .await?,
    )
}

pub async fn chat_bucket(db: &PgPool, chat_id: i64) -> Result<String> {
    sqlx::query_scalar("SELECT bucket FROM chat WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| anyhow!("chat {chat_id} not found"))
}

pub fn row_to_chat_last(row: PgRow) -> ChatLastMessageOutput {
    ChatLastMessageOutput {
        chat_id: row.get("chat_id"),
        chat_name: row.get("chat_name"),
        author_name: row.get("author_name"),
        author_type: row.get("author_type"),
        content: row.get("content"),
        msg_created_at: row.get("msg_created_at"),
        msg_count: row.get("msg_count"),
    }
}

pub fn row_to_message(row: PgRow) -> MessageOutput {
    MessageOutput {
        id: row.get("id"),
        author: row.get("author"),
        author_type: row.get("author_type"),
        content: row.get("content"),
        attachment_name: row.get("attachment_name"),
        created_at: row.get("created_at"),
    }
}
