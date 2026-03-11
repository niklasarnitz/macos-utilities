use ab_glyph::{FontRef, PxScale};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{DateTime, Local, TimeZone};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;

use crate::constants::{FONT_DATA, INFOBAR_H, INFOBAR_W};

#[derive(Clone)]
pub struct NextCalendarEvent {
    pub title: String,
    pub start: DateTime<Local>,
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        input.to_owned()
    } else {
        let mut out = input
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        out.push('…');
        out
    }
}

pub fn render_next_calendar_image(
    event: Option<&NextCalendarEvent>,
    status: Option<&str>,
) -> Option<String> {
    let mut image = RgbaImage::new(INFOBAR_W, INFOBAR_H);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 255]);
    }

    let font = FontRef::try_from_slice(FONT_DATA).ok()?;

    let title_scale = PxScale { x: 18.0, y: 18.0 };
    let subtitle_scale = PxScale { x: 24.0, y: 24.0 };
    let text_color = Rgba([255, 255, 255, 255]);

    if let Some(event) = event {
        draw_text_mut(
            &mut image,
            text_color,
            8,
            4,
            title_scale,
            &font,
            "Next Calendar",
        );
        let line1 = truncate(&event.title, 23);
        let line2 = event.start.format("%a %d %b %H:%M").to_string();
        draw_text_mut(
            &mut image,
            text_color,
            8,
            22,
            subtitle_scale,
            &font,
            &line1,
        );
        draw_text_mut(
            &mut image,
            Rgba([180, 180, 180, 255]),
            8,
            40,
            title_scale,
            &font,
            &line2,
        );
    } else if let Some(status) = status {
        draw_text_mut(
            &mut image,
            text_color,
            8,
            10,
            subtitle_scale,
            &font,
            "Calendar unavailable",
        );
        draw_text_mut(
            &mut image,
            Rgba([180, 180, 180, 255]),
            8,
            35,
            title_scale,
            &font,
            &truncate(status, 30),
        );
    } else {
        draw_text_mut(
            &mut image,
            text_color,
            8,
            10,
            subtitle_scale,
            &font,
            "No upcoming",
        );
        draw_text_mut(
            &mut image,
            Rgba([180, 180, 180, 255]),
            8,
            35,
            title_scale,
            &font,
            "calendar event",
        );
    }

    let mut buffer = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .ok()?;

    Some(format!("data:image/png;base64,{}", B64.encode(buffer.into_inner())))
}

#[cfg(target_os = "macos")]
pub fn get_next_event(selected_calendars: &[String]) -> Result<Option<NextCalendarEvent>, String> {
    use std::process::Command;

    fn escape_applescript_string(input: &str) -> String {
        input.replace('"', "\\\"")
    }

    let selected_literal = if selected_calendars.is_empty() {
        "{}".to_owned()
    } else {
        let values = selected_calendars
            .iter()
            .map(|name| format!("\"{}\"", escape_applescript_string(name)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{{values}}}")
    };

    let script = format!(
        concat!(
            "set selectedCalendars to {selected}\n",
            "set useFilter to ((count of selectedCalendars) > 0)\n",
            "tell application \"Calendar\"\n",
            "  try\n",
            "    count calendars\n",
            "  on error errMsg\n",
            "    return \"ERROR|\" & errMsg\n",
            "  end try\n",
            "\n",
            "  set nowDate to (current date)\n",
            "  set maxDate to nowDate + (30 * days)\n",
            "  set bestStart to missing value\n",
            "  set bestTitle to \"\"\n",
            "\n",
            "  repeat with c in calendars\n",
            "    set calName to (name of c as string)\n",
            "    if (not useFilter) or (selectedCalendars contains calName) then\n",
            "      set eventsList to (every event of c whose start date ≥ nowDate and start date ≤ maxDate)\n",
            "      repeat with e in eventsList\n",
            "        set s to (start date of e)\n",
            "        if (bestStart is missing value) or (s < bestStart) then\n",
            "          set bestStart to s\n",
            "          set t to (summary of e as string)\n",
            "          if t is \"\" then set t to \"(No title)\"\n",
            "          set bestTitle to t\n",
            "        end if\n",
            "      end repeat\n",
            "    end if\n",
            "  end repeat\n",
            "\n",
            "  if bestStart is missing value then\n",
            "    return \"NONE\"\n",
            "  end if\n",
            "\n",
            "  return \"OK|\" & (bestStart as «class isot») & \"|\" & bestTitle\n",
            "end tell"
        ),
        selected = selected_literal
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if err.is_empty() {
            "Calendar unavailable. Please allow Calendar access for OpenDeck when prompted."
                .to_owned()
        } else {
            format!("Calendar unavailable: {err}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout == "NONE" {
        return Ok(None);
    }

    if let Some(rest) = stdout.strip_prefix("ERROR|") {
        return Err(format!(
            "Calendar unavailable: {rest}. Please grant Calendar access when prompted."
        ));
    }

    let mut parts = stdout.splitn(3, '|');
    let status = parts.next().unwrap_or_default();
    if status != "OK" {
        return Err("Calendar unavailable. Please grant Calendar access when prompted.".to_owned());
    }

    let iso = parts
        .next()
        .ok_or_else(|| "Failed to parse calendar response (missing date)".to_owned())?;
    let title = parts
        .next()
        .ok_or_else(|| "Failed to parse calendar response (missing title)".to_owned())?
        .to_owned();

    let start = chrono::DateTime::parse_from_rfc3339(iso)
        .map_err(|e| format!("Invalid calendar date format: {e}"))?
        .with_timezone(&Local);

    Ok(Some(NextCalendarEvent { title, start }))
}

#[cfg(not(target_os = "macos"))]
pub fn get_next_event(_selected_calendars: &[String]) -> Result<Option<NextCalendarEvent>, String> {
    Ok(None)
}
