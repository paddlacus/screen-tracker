use anyhow::Result;
use chrono::Local;
use directories::ProjectDirs;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

fn db_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "screentracker", "ScreenTracker")
        .ok_or_else(|| anyhow::anyhow!("No app data directory"))?;
    std::fs::create_dir_all(dirs.data_dir())?;
    Ok(dirs.data_dir().join("tracker.db"))
}

pub fn open_db() -> Result<Connection> {
    let conn = Connection::open(db_path()?)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_minutes (
            id          INTEGER PRIMARY KEY,
            date        TEXT    NOT NULL,
            app_name    TEXT    NOT NULL,
            is_exempt   INTEGER NOT NULL DEFAULT 0,
            minutes     INTEGER NOT NULL DEFAULT 0,
            UNIQUE(date, app_name, is_exempt)
        );
        CREATE TABLE IF NOT EXISTS daily_meta (
            date            TEXT PRIMARY KEY,
            total_minutes   INTEGER NOT NULL DEFAULT 0,
            exempt_minutes  INTEGER NOT NULL DEFAULT 0,
            warning_sent    INTEGER NOT NULL DEFAULT 0,
            report_sent     INTEGER NOT NULL DEFAULT 0
        );",
    )?;
    Ok(conn)
}

/// Record one minute of usage for the given app.
pub fn record_minute(
    conn: &Connection,
    app_name: &str,
    is_exempt: bool,
) -> Result<()> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let exempt_int = if is_exempt { 1 } else { 0 };

    conn.execute(
        "INSERT INTO app_minutes (date, app_name, is_exempt, minutes)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT(date, app_name, is_exempt) DO UPDATE SET minutes = minutes + 1",
        params![today, app_name, exempt_int],
    )?;

    conn.execute(
        "INSERT INTO daily_meta (date, total_minutes, exempt_minutes)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET
           total_minutes  = total_minutes  + ?2,
           exempt_minutes = exempt_minutes + ?3",
        params![today, if is_exempt { 0 } else { 1 }, if is_exempt { 1 } else { 0 }],
    )?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DailySummary {
    pub date: String,
    pub device_name: String,
    pub total_minutes: i64,
    pub exempt_minutes: i64,
    pub limited_minutes: i64,
    pub app_breakdown: HashMap<String, i64>,
}

pub fn today_summary(conn: &Connection, device_name: &str) -> Result<DailySummary> {
    let today = Local::now().format("%Y-%m-%d").to_string();

    let (total, exempt) = conn
        .query_row(
            "SELECT total_minutes, exempt_minutes FROM daily_meta WHERE date = ?1",
            params![today],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap_or((0, 0));

    let mut stmt = conn.prepare(
        "SELECT app_name, SUM(minutes) FROM app_minutes
         WHERE date = ?1 AND is_exempt = 0
         GROUP BY app_name
         ORDER BY SUM(minutes) DESC",
    )?;
    let app_breakdown: HashMap<String, i64> = stmt
        .query_map(params![today], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(DailySummary {
        date: today,
        device_name: device_name.to_string(),
        total_minutes: total,
        exempt_minutes: exempt,
        limited_minutes: total - exempt,
        app_breakdown,
    })
}

pub fn get_today_limited_minutes(conn: &Connection) -> Result<i64> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let minutes = conn
        .query_row(
            "SELECT total_minutes - exempt_minutes FROM daily_meta WHERE date = ?1",
            params![today],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    Ok(minutes)
}


pub fn was_report_sent_today(conn: &Connection) -> Result<bool> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let sent = conn
        .query_row(
            "SELECT report_sent FROM daily_meta WHERE date = ?1",
            params![today],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    Ok(sent == 1)
}

pub fn mark_report_sent(conn: &Connection) -> Result<()> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO daily_meta (date, report_sent) VALUES (?1, 1)
         ON CONFLICT(date) DO UPDATE SET report_sent = 1",
        params![today],
    )?;
    Ok(())
}
