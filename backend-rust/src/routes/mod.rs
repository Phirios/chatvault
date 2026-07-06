use anyhow::Result;
use axum::{
    extract::{Multipart, Path as AxumPath, Query, State},
    http::StatusCode,
    response::Response,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Local;
use sqlx::Row;
use std::{
    io::{Read, Seek},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

use crate::{
    config::env_path,
    db::{chat_bucket, find_or_create_chat, row_to_chat_last, row_to_message},
    error::AppError,
    models::{
        AttachmentInfoOutput, ChatLastMessageOutput, DateQuery, DateStatisticOutput, MessageOutput,
        MessageStatisticsOutput, NewMessageInput, PageOutput, PageQuery, StatsQuery, VersionOutput,
    },
    parser::MessageParser,
    response::{bytes_response, days_in_month, page_output},
    state::AppState,
    storage::{normalize_prefix, object_key},
};

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/version", get(version))
        .route("/api/chats", get(list_chats))
        .route(
            "/api/chats/{chat_id}",
            get(list_messages).delete(delete_chat),
        )
        .route(
            "/api/chats/{chat_id}/messages-around-date",
            get(messages_around_date),
        )
        .route(
            "/api/chats/{chat_id}/message-statistics",
            get(message_statistics),
        )
        .route(
            "/api/chats/{chat_id}/messages/{message_id}/attachment",
            get(download_attachment),
        )
        .route("/api/chats/{chat_id}/attachments", get(list_attachments))
        .route(
            "/api/chats/{chat_id}/profile-image",
            get(profile_image).post(upload_profile_image),
        )
        .route(
            "/api/chats/{chat_id}/chatName/{chat_name}",
            patch(rename_chat),
        )
        .route(
            "/api/chats/{chat_id}/messages/import",
            post(import_by_chat_id),
        )
        .route("/api/chats/import/{chat_name}", post(import_by_chat_name))
        .route("/api/chats/disk-import", post(disk_import))
        .route("/api/chats/{chat_id}/export", get(export_chat))
        .route("/api/chats/export/all", get(export_all))
        .route("/api/messages", post(new_message))
        .with_state(state)
}

async fn version(State(state): State<AppState>) -> Json<VersionOutput> {
    Json(VersionOutput {
        version: state.config.version,
    })
}

async fn list_chats(
    State(state): State<AppState>,
) -> Result<Json<Vec<ChatLastMessageOutput>>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT c.id AS chat_id, c.name AS chat_name, m.author AS author_name, m.author_type,
               m.content, m.created_at AS msg_created_at,
               (SELECT COUNT(*) FROM message me WHERE me.chat_id = c.id) AS msg_count
        FROM chat c
        JOIN message m ON c.id = m.chat_id
             AND m.id = (SELECT MAX(m2.id) FROM message m2 WHERE m2.chat_id = c.id)
        ORDER BY m.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(row_to_chat_last).collect()))
}

async fn list_messages(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PageOutput<MessageOutput>>, AppError> {
    let page = query.page.unwrap_or(0).max(0);
    let size = query.size.unwrap_or(20).clamp(1, 2000);
    let search = query.query.unwrap_or_default();
    let like = format!("%{}%", search.to_lowercase());

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM message
        WHERE chat_id = $1
          AND ($2 = '' OR LOWER(author) LIKE $3 OR LOWER(content) LIKE $3)
        "#,
    )
    .bind(chat_id)
    .bind(&search)
    .bind(&like)
    .fetch_one(&state.db)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT id, author, author_type, content, attachment_name, created_at
        FROM message
        WHERE chat_id = $1
          AND ($2 = '' OR LOWER(author) LIKE $3 OR LOWER(content) LIKE $3)
        ORDER BY created_at DESC, id DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(chat_id)
    .bind(&search)
    .bind(&like)
    .bind(size)
    .bind(page * size)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(page_output(
        rows.into_iter().map(row_to_message).collect(),
        page,
        size,
        total,
    )))
}

async fn messages_around_date(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
    Query(query): Query<DateQuery>,
) -> Result<Json<PageOutput<MessageOutput>>, AppError> {
    let size = query.page_size.unwrap_or(20).clamp(1, 2000);
    let target = query
        .date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::bad_request("invalid date"))?;
    let rows = sqlx::query(
        r#"
        SELECT id, author, author_type, content, attachment_name, created_at
        FROM message
        WHERE chat_id = $1 AND created_at >= $2
        ORDER BY created_at ASC, id ASC
        LIMIT $3
        "#,
    )
    .bind(chat_id)
    .bind(target)
    .bind(size)
    .fetch_all(&state.db)
    .await?;
    let len = rows.len() as i64;
    Ok(Json(page_output(
        rows.into_iter().map(row_to_message).collect(),
        0,
        size,
        len,
    )))
}

async fn message_statistics(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<MessageStatisticsOutput>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT DATE(created_at) AS date, COUNT(*) AS count
        FROM message
        WHERE chat_id = $1
          AND EXTRACT(YEAR FROM created_at) = $2
          AND EXTRACT(MONTH FROM created_at) = $3
        GROUP BY DATE(created_at)
        ORDER BY DATE(created_at) ASC
        "#,
    )
    .bind(chat_id)
    .bind(query.year as f64)
    .bind(query.month as f64)
    .fetch_all(&state.db)
    .await?;

    let statistics: Vec<_> = rows
        .into_iter()
        .map(|row| DateStatisticOutput {
            date: row.get("date"),
            count: row.get::<i64, _>("count"),
        })
        .collect();

    let first = chrono::NaiveDate::from_ymd_opt(query.year, query.month as u32, 1)
        .ok_or_else(|| AppError::bad_request("invalid month"))?;
    let max_day = days_in_month(query.year, query.month as u32)?;
    let last = chrono::NaiveDate::from_ymd_opt(query.year, query.month as u32, max_day).unwrap();
    let days_with_messages = statistics.len();

    Ok(Json(MessageStatisticsOutput {
        month: query.month,
        year: query.year,
        statistics,
        min_date: first,
        max_date: last,
        is_data_dense: days_with_messages > 15,
    }))
}

async fn list_attachments(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
) -> Result<Json<Vec<AttachmentInfoOutput>>, AppError> {
    let rows = sqlx::query(
        "SELECT id, attachment_name FROM message WHERE chat_id = $1 AND attachment_path IS NOT NULL ORDER BY id ASC",
    )
    .bind(chat_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| AttachmentInfoOutput {
                id: row.get("id"),
                name: row
                    .get::<Option<String>, _>("attachment_name")
                    .unwrap_or_default(),
            })
            .collect(),
    ))
}

async fn download_attachment(
    State(state): State<AppState>,
    AxumPath((chat_id, message_id)): AxumPath<(i64, i64)>,
) -> Result<Response, AppError> {
    let row = sqlx::query(
        "SELECT attachment_path, attachment_name FROM message WHERE chat_id = $1 AND id = $2",
    )
    .bind(chat_id)
    .bind(message_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("message not found"))?;
    let key: String = row
        .get::<Option<String>, _>("attachment_path")
        .ok_or_else(|| AppError::not_found("attachment not found"))?;
    let filename = row
        .get::<Option<String>, _>("attachment_name")
        .unwrap_or_else(|| "attachment".to_string());
    bytes_response(state.storage.get(&key).await?, Some(filename))
}

async fn profile_image(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
) -> Result<Response, AppError> {
    let key = format!("{chat_id}/profile-image");
    match state.storage.get(&key).await {
        Ok(bytes) => bytes_response(bytes, Some("profile-image".to_string())),
        Err(_) => Err(AppError::not_found("profile image not found")),
    }
}

async fn upload_profile_image(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("file") {
            let bytes = field.bytes().await?.to_vec();
            state
                .storage
                .put(&format!("{chat_id}/profile-image"), bytes)
                .await?;
            return Ok(StatusCode::NO_CONTENT);
        }
    }
    Err(AppError::bad_request("missing file field"))
}

async fn rename_chat(
    State(state): State<AppState>,
    AxumPath((chat_id, chat_name)): AxumPath<(i64, String)>,
) -> Result<StatusCode, AppError> {
    let decoded = urlencoding::decode(&chat_name)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| chat_name.clone());
    sqlx::query("UPDATE chat SET name = $1 WHERE id = $2")
        .bind(decoded)
        .bind(chat_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_chat(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
) -> Result<StatusCode, AppError> {
    let bucket: Option<String> = sqlx::query_scalar("SELECT bucket FROM chat WHERE id = $1")
        .bind(chat_id)
        .fetch_optional(&state.db)
        .await?;
    if let Some(prefix) = bucket {
        state
            .storage
            .delete_prefix(&normalize_prefix(&prefix))
            .await?;
    }
    sqlx::query("DELETE FROM chat WHERE id = $1")
        .bind(chat_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn new_message(
    State(state): State<AppState>,
    Json(input): Json<NewMessageInput>,
) -> Result<StatusCode, AppError> {
    insert_new_message(&state, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn import_by_chat_id(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
    multipart: Multipart,
) -> Result<StatusCode, AppError> {
    import_multipart(state, Some(chat_id), None, multipart).await
}

async fn import_by_chat_name(
    State(state): State<AppState>,
    AxumPath(chat_name): AxumPath<String>,
    multipart: Multipart,
) -> Result<StatusCode, AppError> {
    import_multipart(state, None, Some(chat_name), multipart).await
}

async fn import_multipart(
    state: AppState,
    chat_id: Option<i64>,
    chat_name: Option<String>,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    while let Some(mut field) = multipart.next_field().await? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("chat.txt").to_string();
            let upload_path = state
                .config
                .scratch_dir
                .join(format!("{}.upload", Uuid::new_v4()));
            let mut file = fs::File::create(&upload_path).await?;
            while let Some(chunk) = field.chunk().await? {
                file.write_all(&chunk).await?;
            }
            file.flush().await?;
            drop(file);
            let chat_id = match chat_id {
                Some(id) => id,
                None => {
                    find_or_create_chat(&state.db, chat_name.as_deref().unwrap_or("Imported Chat"))
                        .await?
                }
            };
            let result = import_file(&state, chat_id, &upload_path, &filename).await;
            let _ = fs::remove_file(&upload_path).await;
            result?;
            return Ok(StatusCode::NO_CONTENT);
        }
    }
    Err(AppError::bad_request("missing file field"))
}

async fn disk_import(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    let import_dir = env_path("CHATVAULT_IMPORT_DIR", "/opt/chatvault/import");
    if !fs::try_exists(&import_dir).await? {
        return Ok(StatusCode::NO_CONTENT);
    }
    let mut dirs = fs::read_dir(import_dir).await?;
    while let Some(entry) = dirs.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            let filename = entry.file_name().to_string_lossy().to_string();
            let chat_name = filename.trim_end_matches(".zip").trim_end_matches(".txt");
            let chat_id = find_or_create_chat(&state.db, chat_name).await?;
            import_file(&state, chat_id, &path, &filename).await?;
            let _ = fs::remove_file(path).await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn import_file_for_chat_name(
    state: &AppState,
    chat_name: &str,
    path: &Path,
    filename: &str,
) -> Result<(), AppError> {
    let chat_id = find_or_create_chat(&state.db, chat_name).await?;
    import_file(state, chat_id, path, filename).await
}

async fn export_chat(
    State(state): State<AppState>,
    AxumPath(chat_id): AxumPath<i64>,
) -> Result<Response, AppError> {
    let bucket: String = sqlx::query_scalar("SELECT bucket FROM chat WHERE id = $1")
        .bind(chat_id)
        .fetch_one(&state.db)
        .await?;
    export_prefix(
        &state,
        normalize_prefix(&bucket),
        format!("chat-{chat_id}.zip"),
    )
    .await
}

async fn export_all(State(state): State<AppState>) -> Result<Response, AppError> {
    export_prefix(&state, "".to_string(), "chatvault.zip".to_string()).await
}

async fn export_prefix(
    state: &AppState,
    prefix: String,
    filename: String,
) -> Result<Response, AppError> {
    let keys = state.storage.list_prefix(&prefix).await?;
    let mut zip_file = NamedTempFile::new_in(&state.config.scratch_dir)?;
    {
        let mut zip = ZipWriter::new(&mut zip_file);
        let options = SimpleFileOptions::default();
        for key in keys {
            let bytes = state.storage.get(&key).await?;
            let entry = key
                .strip_prefix(&prefix)
                .unwrap_or(&key)
                .trim_start_matches('/');
            zip.start_file(entry, options)?;
            std::io::Write::write_all(&mut zip, &bytes)?;
        }
        zip.finish()?;
    }
    let bytes = fs::read(zip_file.path()).await?;
    bytes_response(bytes, Some(filename))
}

async fn import_file(
    state: &AppState,
    chat_id: i64,
    path: &Path,
    filename: &str,
) -> Result<(), AppError> {
    if filename.to_lowercase().ends_with(".zip") {
        let file = std::fs::File::open(path)?;
        import_zip(state, chat_id, file).await?;
    } else {
        let text = fs::read_to_string(path).await?;
        import_text(state, chat_id, filename, text).await?;
    }
    Ok(())
}

async fn import_zip<R: Read + Seek>(
    state: &AppState,
    chat_id: i64,
    reader: R,
) -> Result<(), AppError> {
    let entries = extract_zip_entries(reader, &state.config.scratch_dir)?;
    let bucket = chat_bucket(&state.db, chat_id).await?;

    for (name, path) in entries {
        let key = object_key(&bucket, &name)?;
        state.storage.put_file(&key, &path).await?;
        if is_chat_text_file(&name) {
            let text = fs::read_to_string(&path).await?;
            import_text(state, chat_id, &name, text).await?;
        }
        let _ = fs::remove_file(path).await;
    }
    Ok(())
}

fn extract_zip_entries<R: Read + Seek>(
    reader: R,
    scratch_dir: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    let mut archive = ZipArchive::new(reader)?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let path = scratch_dir.join(format!("{}.entry", Uuid::new_v4()));
        let mut output = std::fs::File::create(&path)?;
        std::io::copy(&mut file, &mut output)?;
        entries.push((name, path));
    }
    Ok(entries)
}

async fn import_text(
    state: &AppState,
    chat_id: i64,
    filename: &str,
    text: String,
) -> Result<(), AppError> {
    let bucket = chat_bucket(&state.db, chat_id).await?;
    state
        .storage
        .put(&object_key(&bucket, filename)?, text.as_bytes().to_vec())
        .await?;
    let parser = MessageParser::new(state.config.msg_date_format.clone())?;
    let parsed = parser.parse_messages(&text);
    let mut tx = state.db.begin().await?;
    for message in parsed {
        let attachment_path = message
            .attachment_name
            .as_ref()
            .map(|name| object_key(&bucket, name))
            .transpose()?;
        sqlx::query(
            r#"
            INSERT INTO message(author, author_type, created_at, content, attachment_path, attachment_name, chat_id, external_id)
            SELECT $1, $2, $3, $4, $5, $6, $7, $8
            WHERE NOT EXISTS (
                SELECT 1 FROM message
                WHERE chat_id = $7 AND created_at = $3 AND author = $1 AND content = $4
            )
            ON CONFLICT (external_id) DO NOTHING
            "#,
        )
        .bind(&message.author)
        .bind(&message.author_type)
        .bind(message.created_at)
        .bind(&message.content)
        .bind(attachment_path)
        .bind(&message.attachment_name)
        .bind(chat_id)
        .bind(Option::<String>::None)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn insert_new_message(state: &AppState, input: NewMessageInput) -> Result<()> {
    let attachment_name = input.attachment.as_ref().map(|a| a.name.clone());
    let attachment_path = if let Some(attachment) = &input.attachment {
        let bucket = chat_bucket(&state.db, input.chat_id).await?;
        let key = object_key(&bucket, &attachment.name)?;
        state
            .storage
            .put(&key, attachment.content.as_bytes().to_vec())
            .await?;
        Some(key)
    } else {
        None
    };
    sqlx::query(
        r#"
        INSERT INTO message(author, author_type, created_at, content, attachment_path, attachment_name, chat_id, external_id)
        SELECT $1, 'USER', $2, $3, $4, $5, $6, $7
        WHERE NOT EXISTS (
            SELECT 1 FROM message
            WHERE chat_id = $6 AND created_at = $2 AND author = $1 AND content = $3
        )
        ON CONFLICT (external_id) DO NOTHING
        "#,
    )
    .bind(input.author_name)
    .bind(input.created_at.unwrap_or_else(|| Local::now().naive_local()))
    .bind(input.content)
    .bind(attachment_path)
    .bind(attachment_name)
    .bind(input.chat_id)
    .bind(input.external_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn is_chat_text_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".txt")
        && (lower.contains("whatsapp") || lower.contains("chat") || lower.contains("conversa"))
}
