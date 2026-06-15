use anyhow::Result;
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::storage::DailySummary;

pub struct EmailConfig {
    pub gmail_address: String,
    pub gmail_app_password: String,
    pub recipient: String,
}

fn format_minutes(mins: i64) -> String {
    let h = mins / 60;
    let m = mins % 60;
    if h == 0 {
        format!("{m}m")
    } else if m == 0 {
        format!("{h}h")
    } else {
        format!("{h}h {m}m")
    }
}

fn build_mailer(cfg: &EmailConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let creds = Credentials::new(cfg.gmail_address.clone(), cfg.gmail_app_password.clone());
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.gmail.com")?
        .credentials(creds)
        .build();
    Ok(mailer)
}

/// Send the 2.5-hour (or custom limit) warning email.
pub async fn send_warning_email(cfg: &EmailConfig, summary: &DailySummary, limit_hours: f64) -> Result<()> {
    let subject = format!(
        "⚠️ Screen time warning — {} has reached {}h limit",
        summary.device_name,
        limit_hours
    );
    let body = format!(
        r#"<html><body style="font-family:sans-serif;max-width:480px;margin:auto;">
<h2 style="color:#e05c00;">Screen Time Warning</h2>
<p><strong>{device}</strong> has crossed the <strong>{limit_h:.1}h daily limit</strong>.</p>
<p>Time used so far today: <strong>{used}</strong></p>
<p style="color:#888;font-size:12px;">This email is sent once per day when the threshold is crossed.<br>
Exempt (school-hours) time is not counted toward the limit.</p>
</body></html>"#,
        device = summary.device_name,
        limit_h = limit_hours,
        used = format_minutes(summary.total_minutes),
    );
    send_html(cfg, &subject, &body).await
}

/// Send the end-of-day report email combining one or two machine summaries.
pub async fn send_daily_report(
    cfg: &EmailConfig,
    summaries: &[DailySummary],
    limit_hours: f64,
) -> Result<()> {
    let date = summaries
        .first()
        .map(|s| s.date.as_str())
        .unwrap_or("today");
    let subject = format!("📊 Daily screen time report — {date}");

    let mut machines_html = String::new();
    for s in summaries {
        let over = s.total_minutes as f64 / 60.0 > limit_hours;
        let color = if over { "#c0392b" } else { "#27ae60" };
        machines_html.push_str(&format!(
            r#"<div style="border:1px solid #ddd;border-radius:8px;padding:16px;margin-bottom:16px;">
<h3 style="margin:0 0 8px;">{device}</h3>
<p style="margin:4px 0;">Tracked time: <strong style="color:{color};">{used}</strong> / {limit_h:.1}h limit</p>
<p style="margin:4px 0;color:#888;font-size:13px;">Exempt (school hours): {exempt}</p>"#,
            device = s.device_name,
            color = color,
            used = format_minutes(s.total_minutes),
            limit_h = limit_hours,
            exempt = format_minutes(s.exempt_minutes),
        ));

        if !s.app_breakdown.is_empty() {
            machines_html.push_str("<h4 style=\"margin:12px 0 4px;\">Top apps</h4><table style=\"width:100%;border-collapse:collapse;font-size:13px;\">");
            let mut apps: Vec<_> = s.app_breakdown.iter().collect();
            apps.sort_by(|a, b| b.1.cmp(a.1));
            for (app, mins) in apps.iter().take(10) {
                machines_html.push_str(&format!(
                    "<tr><td style=\"padding:2px 0;\">{app}</td><td style=\"text-align:right;color:#555;\">{time}</td></tr>",
                    app = app,
                    time = format_minutes(**mins),
                ));
            }
            machines_html.push_str("</table>");
        }
        machines_html.push_str("</div>");
    }

    let body = format!(
        r#"<html><body style="font-family:sans-serif;max-width:520px;margin:auto;">
<h2>Daily Screen Time Report</h2>
<p style="color:#888;">{date}</p>
{machines}
<p style="font-size:11px;color:#aaa;margin-top:24px;">
Sent automatically at end of day by Screen Tracker.
App time = foreground time only. Exempt windows (school hours) excluded from limit tracking.
</p>
</body></html>"#,
        date = date,
        machines = machines_html,
    );
    send_html(cfg, &subject, &body).await
}

async fn send_html(cfg: &EmailConfig, subject: &str, html_body: &str) -> Result<()> {
    let email = Message::builder()
        .from(cfg.gmail_address.parse()?)
        .to(cfg.recipient.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html_body.to_string())?;

    let mailer = build_mailer(cfg)?;
    mailer.send(email).await?;
    Ok(())
}
