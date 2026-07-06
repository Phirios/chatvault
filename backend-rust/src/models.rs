use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct VersionOutput {
    pub version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLastMessageOutput {
    pub chat_id: i64,
    pub chat_name: String,
    pub author_name: String,
    pub author_type: String,
    pub content: String,
    pub msg_created_at: NaiveDateTime,
    pub msg_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageOutput {
    pub id: i64,
    pub author: String,
    pub author_type: String,
    pub content: String,
    pub attachment_name: Option<String>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfoOutput {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct PageOutput<T> {
    pub content: Vec<T>,
    pub last: bool,
    pub first: bool,
    pub number: i64,
    pub size: i64,
    #[serde(rename = "totalElements")]
    pub total_elements: i64,
    #[serde(rename = "totalPages")]
    pub total_pages: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStatisticsOutput {
    pub month: i32,
    pub year: i32,
    pub statistics: Vec<DateStatisticOutput>,
    pub min_date: NaiveDate,
    pub max_date: NaiveDate,
    pub is_data_dense: bool,
}

#[derive(Debug, Serialize)]
pub struct DateStatisticOutput {
    pub date: NaiveDate,
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub page: Option<i64>,
    pub size: Option<i64>,
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateQuery {
    pub date: NaiveDate,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub year: i32,
    pub month: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMessageInput {
    pub author_name: String,
    pub chat_id: i64,
    pub external_id: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub content: String,
    pub attachment: Option<NewAttachmentInput>,
}

#[derive(Debug, Deserialize)]
pub struct NewAttachmentInput {
    pub name: String,
    pub content: String,
}

#[derive(Debug)]
pub struct ParsedMessage {
    pub author: String,
    pub author_type: String,
    pub created_at: NaiveDateTime,
    pub content: String,
    pub attachment_name: Option<String>,
}
