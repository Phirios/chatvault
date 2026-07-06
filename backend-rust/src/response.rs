use anyhow::Result;
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
};
use chrono::{Datelike, NaiveDate};

use crate::{error::AppError, models::PageOutput};

pub fn page_output<T>(content: Vec<T>, page: i64, size: i64, total: i64) -> PageOutput<T> {
    let total_pages = if total == 0 {
        0
    } else {
        (total + size - 1) / size
    };
    PageOutput {
        first: page == 0,
        last: page + 1 >= total_pages,
        number: page,
        size,
        total_elements: total,
        total_pages,
        content,
    }
}

pub fn bytes_response(bytes: Vec<u8>, filename: Option<String>) -> Result<Response, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Some(filename) = filename {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!(
                "attachment; filename=\"{}\"",
                filename.replace('"', "")
            ))
            .unwrap(),
        );
    }
    Ok((headers, Body::from(bytes)).into_response())
}

pub fn days_in_month(year: i32, month: u32) -> Result<u32, AppError> {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| AppError::bad_request("invalid month"))?;
    Ok((next - chrono::Duration::days(1)).day())
}
