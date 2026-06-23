use chrono::{DateTime, Utc};

use crate::domain::meeting::{MeetingBundle, MeetingError};

pub fn meeting_invite_ics(
    bundle: &MeetingBundle,
    public_url: &str,
    generated_at: DateTime<Utc>,
) -> Result<String, MeetingError> {
    let join_url = join_url(public_url, &bundle.meeting.join_slug);
    let description = match &bundle.meeting.description {
        Some(description) => format!("{description}\n\nJoin: {join_url}"),
        None => format!("Join: {join_url}"),
    };
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        "PRODID:-//OpenCord//OpenCord Calendar//EN".to_owned(),
        "CALSCALE:GREGORIAN".to_owned(),
        "METHOD:PUBLISH".to_owned(),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}@opencord", bundle.meeting.id),
        format!("DTSTAMP:{}", format_ics_time(generated_at)),
        format!(
            "DTSTART:{}",
            meeting_time(&bundle.meeting.starts_at, "meeting starts_at is invalid")?
        ),
        format!(
            "DTEND:{}",
            meeting_time(&bundle.meeting.ends_at, "meeting ends_at is invalid")?
        ),
        format!("SUMMARY:{}", escape_text(&bundle.meeting.title)),
        format!("DESCRIPTION:{}", escape_text(&description)),
        format!("LOCATION:{}", escape_text(&join_url)),
        format!("URL:{join_url}"),
        format!(
            "STATUS:{}",
            if bundle.meeting.status == "cancelled" {
                "CANCELLED"
            } else {
                "CONFIRMED"
            }
        ),
    ];

    for attendee in &bundle.attendees {
        let Some(email) = &attendee.email else {
            continue;
        };
        let role = match attendee.role.as_str() {
            "host" => "CHAIR",
            "optional" => "OPT-PARTICIPANT",
            _ => "REQ-PARTICIPANT",
        };
        let common_name = attendee.display_name.as_deref().unwrap_or(email.as_str());
        lines.push(format!(
            "ATTENDEE;ROLE={role};CN={}:mailto:{email}",
            escape_parameter(common_name)
        ));
    }

    lines.push("END:VEVENT".to_owned());
    lines.push("END:VCALENDAR".to_owned());

    Ok(format!("{}\r\n", lines.join("\r\n")))
}

fn meeting_time(value: &str, message: &'static str) -> Result<String, MeetingError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| format_ics_time(time.with_timezone(&Utc)))
        .map_err(|_| MeetingError::InvalidInput(message))
}

fn format_ics_time(time: DateTime<Utc>) -> String {
    time.format("%Y%m%dT%H%M%SZ").to_string()
}

fn join_url(public_url: &str, join_slug: &str) -> String {
    let public_url = public_url.trim_end_matches('/');
    if public_url.is_empty() {
        format!("/join/{join_slug}")
    } else {
        format!("{public_url}/join/{join_slug}")
    }
}

fn escape_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace(['\n', '\r'], "\\n")
}

fn escape_parameter(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace(['\r', '\n'], " ")
    )
}
