use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;

use crate::config::DriveConfig;

/// Write a file to Drive via the Apps Script proxy.
pub async fn upload_file(
    script_url: &str,
    secret: &str,
    filename: &str,
    content: &str,
) -> Result<()> {
    let client = Client::new();
    let body = serde_json::json!({
        "secret": secret,
        "filename": filename,
        "content": content,
    });
    let resp: Value = client
        .post(script_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Network error: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("Response parse error: {e}"))?;

    if resp["ok"].as_bool() != Some(true) {
        let err = resp["error"].as_str().unwrap_or("unknown error");
        return Err(anyhow!("Script error: {err}"));
    }
    Ok(())
}

/// Read a file from Drive via the Apps Script proxy. Returns None if not found.
pub async fn download_file(
    script_url: &str,
    secret: &str,
    filename: &str,
) -> Result<Option<String>> {
    let client = Client::new();
    let resp: Value = client
        .get(script_url)
        .query(&[("secret", secret), ("filename", filename)])
        .send()
        .await
        .map_err(|e| anyhow!("Network error: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("Response parse error: {e}"))?;

    if resp["error"].as_str() == Some("not_found") {
        return Ok(None);
    }
    if resp["ok"].as_bool() != Some(true) {
        let err = resp["error"].as_str().unwrap_or("unknown error");
        return Err(anyhow!("Script error: {err}"));
    }
    Ok(resp["content"].as_str().map(|s| s.to_string()))
}

/// Fetch and parse config.json. Returns None if not found (caller uses defaults).
pub async fn fetch_config(script_url: &str, secret: &str) -> Result<Option<DriveConfig>> {
    match download_file(script_url, secret, "config.json").await? {
        None => Ok(None),
        Some(text) => Ok(Some(
            serde_json::from_str(&text).map_err(|e| anyhow!("Config parse error: {e}"))?,
        )),
    }
}

/// Upload today's summary JSON. Creates or overwrites the daily file.
pub async fn push_daily_summary(
    script_url: &str,
    secret: &str,
    device_name: &str,
    summary_json: &str,
) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let safe_name: String = device_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let filename = format!("{safe_name}_{today}.json");
    upload_file(script_url, secret, &filename, summary_json).await
}

/// List all filenames in the Drive folder.
pub async fn list_files(script_url: &str, secret: &str) -> Result<Vec<String>> {
    let client = Client::new();
    let resp: Value = client
        .get(script_url)
        .query(&[("secret", secret), ("action", "list")])
        .send()
        .await
        .map_err(|e| anyhow!("Network error: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("Response parse error: {e}"))?;

    if resp["ok"].as_bool() != Some(true) {
        return Err(anyhow!("Script error"));
    }
    Ok(resp["files"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

/// Fetch combined limited_minutes from all devices for today.
pub async fn fetch_combined_today_minutes(script_url: &str, secret: &str) -> Result<i64> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let files = list_files(script_url, secret).await?;
    let today_files: Vec<String> = files
        .into_iter()
        .filter(|f| f.ends_with(&format!("_{today}.json")) && f != "config.json")
        .collect();

    let mut total: i64 = 0;
    for filename in today_files {
        if let Ok(Some(text)) = download_file(script_url, secret, &filename).await {
            if let Ok(summary) = serde_json::from_str::<crate::storage::DailySummary>(&text) {
                total += summary.limited_minutes;
            }
        }
    }
    Ok(total)
}

/// Check if a named flag file exists on Drive.
pub async fn check_flag(script_url: &str, secret: &str, flag: &str) -> bool {
    download_file(script_url, secret, flag)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// Create a named flag file on Drive.
pub async fn set_flag(script_url: &str, secret: &str, flag: &str) -> Result<()> {
    upload_file(script_url, secret, flag, "1").await
}

/// Write the default config.json if it doesn't exist or is empty.
pub async fn ensure_config_exists(
    script_url: &str,
    secret: &str,
    default_config: &DriveConfig,
) -> Result<()> {
    let existing = download_file(script_url, secret, "config.json").await?;
    let is_empty = existing.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true);
    if is_empty {
        let json = serde_json::to_string_pretty(default_config)?;
        upload_file(script_url, secret, "config.json", &json).await?;
    }
    Ok(())
}
