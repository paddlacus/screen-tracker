use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Loaded from Drive config.json — all admin settings live here.
/// The user (you, the admin) edits this file; apps pick up changes within 1 hour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    /// Human-readable instructions embedded in the JSON so you know what each field does.
    #[serde(default)]
    pub _instructions: HashMap<String, String>,

    /// Daily screen-time limit in hours. Decimals ok (e.g. 2.5 = 2h 30m).
    /// The warning email fires once per day when this threshold is crossed.
    pub limit_hours: f64,

    /// When true, minutes where there has been no keyboard/mouse input for
    /// idle_threshold_minutes are NOT counted toward the daily limit.
    pub idle_detection_enabled: bool,

    /// How many consecutive minutes of inactivity before the session is considered idle.
    /// Only used when idle_detection_enabled is true. Recommended: 5.
    pub idle_threshold_minutes: u64,

    /// Default weekly schedule. Keys are lowercase day names: monday..sunday.
    /// Each day lists time windows that are EXEMPT from the limit (e.g. school hours).
    /// Leave the list empty for days with no exemption (full limit applies).
    pub school_schedule: HashMap<String, Vec<TimeWindow>>,

    /// Date-specific overrides that take priority over school_schedule.
    /// Use these for holidays, PD days, summer breaks, etc.
    /// Each override replaces the exempt windows for the specified date(s).
    /// An empty exempt list means NO exemption — the limit applies all day.
    pub overrides: Vec<DateOverride>,

    /// Email address to send warning and daily report emails to.
    pub report_email: String,

    /// Hour (24h) at which the daily report email is sent. Default: 23 (11 PM).
    pub daily_report_hour: u32,

    /// Password required to open the Setup screen. Change this in config.json.
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
}

fn default_admin_password() -> String {
    "admin".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start of exempt window, 24h format "HH:MM"
    pub start: String,
    /// End of exempt window, 24h format "HH:MM"
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateOverride {
    /// Human label so you remember what this is for (e.g. "Winter break", "PD Day")
    pub label: String,

    /// Override a single date: "YYYY-MM-DD". Use either this OR date_range, not both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,

    /// Override a range of dates (inclusive). Use either this OR date, not both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range: Option<DateRange>,

    /// Exempt windows for these date(s). Empty list = no exemption, limit applies all day.
    pub exempt: Vec<TimeWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

impl DriveConfig {
    /// Returns the exempt time windows that apply today.
    pub fn exempt_windows_today(&self) -> Vec<TimeWindow> {
        let today = Local::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();

        // Check overrides first (highest priority)
        for ov in &self.overrides {
            if let Some(ref d) = ov.date {
                if *d == today_str {
                    return ov.exempt.clone();
                }
            }
            if let Some(ref range) = ov.date_range {
                if let (Ok(from), Ok(to)) = (
                    NaiveDate::parse_from_str(&range.from, "%Y-%m-%d"),
                    NaiveDate::parse_from_str(&range.to, "%Y-%m-%d"),
                ) {
                    if today >= from && today <= to {
                        return ov.exempt.clone();
                    }
                }
            }
        }

        // Fall back to weekly schedule
        let day_key = match today.weekday() {
            Weekday::Mon => "monday",
            Weekday::Tue => "tuesday",
            Weekday::Wed => "wednesday",
            Weekday::Thu => "thursday",
            Weekday::Fri => "friday",
            Weekday::Sat => "saturday",
            Weekday::Sun => "sunday",
        };
        self.school_schedule
            .get(day_key)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns true if the current wall-clock time falls within any exempt window today.
    pub fn is_currently_exempt(&self) -> bool {
        let now = Local::now().time();
        for window in self.exempt_windows_today() {
            if let (Ok(start), Ok(end)) = (
                NaiveTime::parse_from_str(&window.start, "%H:%M"),
                NaiveTime::parse_from_str(&window.end, "%H:%M"),
            ) {
                if now >= start && now < end {
                    return true;
                }
            }
        }
        false
    }

    pub fn limit_minutes(&self) -> u64 {
        (self.limit_hours * 60.0).round() as u64
    }
}

impl Default for DriveConfig {
    fn default() -> Self {
        let mut instructions = HashMap::new();
        instructions.insert("limit_hours".into(), "Daily screen-time limit in hours. Decimals ok (e.g. 2.5 = 2h 30m). Warning email fires once per day when crossed.".into());
        instructions.insert("idle_detection_enabled".into(), "Set to true to pause counting when there is no keyboard/mouse input for idle_threshold_minutes.".into());
        instructions.insert("idle_threshold_minutes".into(), "Minutes of inactivity before session is considered idle. Only used when idle_detection_enabled is true.".into());
        instructions.insert("school_schedule".into(), "Default weekly exempt windows. Keys: monday..sunday. Time format HH:MM (24h). Empty list = no exemption that day.".into());
        instructions.insert("overrides".into(), "Date-specific overrides (highest priority). Use 'date' for a single day or 'date_range' for a span. Empty exempt list = limit applies all day.".into());
        instructions.insert("report_email".into(), "Email address that receives warning and daily report emails.".into());
        instructions.insert("daily_report_hour".into(), "Hour (24h, 0-23) when the end-of-day report is sent. Default 23 = 11 PM.".into());
        instructions.insert("admin_password".into(), "Password required to open the Setup screen. Change this to something only you know.".into());

        let mut school_schedule = HashMap::new();
        for day in &["monday", "tuesday", "wednesday", "thursday", "friday"] {
            school_schedule.insert(
                day.to_string(),
                vec![TimeWindow {
                    start: "09:00".into(),
                    end: "15:00".into(),
                }],
            );
        }
        school_schedule.insert("saturday".into(), vec![]);
        school_schedule.insert("sunday".into(), vec![]);

        Self {
            _instructions: instructions,
            limit_hours: 2.5,
            idle_detection_enabled: false,
            idle_threshold_minutes: 5,
            school_schedule,
            overrides: vec![],
            report_email: String::new(),
            daily_report_hour: 23,
            admin_password: "admin".into(),
        }
    }
}

/// Credentials stored locally on the machine (never in Drive).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalCredentials {
    /// The deployed Google Apps Script web app URL (from Deploy → Manage deployments).
    pub script_url: String,

    /// A secret string you chose when setting up the Apps Script — prevents unauthorized access.
    pub script_secret: String,

    /// Gmail address used for sending reports (must have 2FA + App Password enabled).
    pub gmail_address: String,

    /// 16-character Gmail App Password (Google Account → Security → App passwords).
    pub gmail_app_password: String,

    /// Human-readable name for this machine, shown in reports (e.g. "Mac", "Windows").
    pub device_name: String,
}

impl LocalCredentials {
    pub fn is_complete(&self) -> bool {
        !self.script_url.is_empty()
            && !self.script_secret.is_empty()
            && !self.gmail_address.is_empty()
            && !self.gmail_app_password.is_empty()
            && !self.device_name.is_empty()
    }
}

pub fn credentials_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "screentracker", "ScreenTracker")
        .ok_or_else(|| anyhow::anyhow!("Could not find app data directory"))?;
    std::fs::create_dir_all(dirs.data_dir())?;
    Ok(dirs.data_dir().join("credentials.json"))
}

pub fn load_credentials() -> Result<LocalCredentials> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(LocalCredentials::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_credentials(creds: &LocalCredentials) -> Result<()> {
    let path = credentials_path()?;
    std::fs::write(path, serde_json::to_string_pretty(creds)?)?;
    Ok(())
}
