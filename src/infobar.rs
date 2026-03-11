use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::Local;
use image::{Rgba, RgbaImage};
use imageproc::{
    drawing::{draw_filled_circle_mut, draw_filled_rect_mut, draw_text_mut},
    rect::Rect,
};

use crate::constants::{FONT_DATA, INFOBAR_H, INFOBAR_W};
use crate::system_media::{BatteryStatus, NowPlaying, PlaybackProgress};

#[derive(Clone)]
pub struct NowPlayingRenderOptions {
    pub show_cover: bool,
    pub title_font_size: f32,
    pub artist_font_size: f32,
    pub line_clamp: u8,
    pub left_padding: i32,
    pub right_padding: i32,
    pub progress_bar_height: u32,
    pub high_contrast: bool,
    pub rounded_bar: bool,
}

impl Default for NowPlayingRenderOptions {
    fn default() -> Self {
        Self {
            show_cover: true,
            title_font_size: 22.0,
            artist_font_size: 18.0,
            line_clamp: 2,
            left_padding: 10,
            right_padding: 6,
            progress_bar_height: 6,
            high_contrast: false,
            rounded_bar: false,
        }
    }
}

#[derive(Clone, Copy)]
pub enum ProgressTimeFormat {
    Remaining,
    ElapsedTotal,
}

#[derive(Clone)]
pub struct NowPlayingProgressOptions {
    pub base: NowPlayingRenderOptions,
    pub time_format: ProgressTimeFormat,
    pub fallback_mode: String,
}

impl Default for NowPlayingProgressOptions {
    fn default() -> Self {
        Self {
            base: NowPlayingRenderOptions::default(),
            time_format: ProgressTimeFormat::Remaining,
            fallback_mode: "no_track".to_owned(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum BatteryDisplayMode {
    PercentOnly,
    PercentAndCharging,
    IconAndPercent,
}

#[derive(Clone)]
pub struct BatteryRenderOptions {
    pub display_mode: BatteryDisplayMode,
    pub low_battery_threshold: f32,
    pub charging_indicator_style: String,
}

impl Default for BatteryRenderOptions {
    fn default() -> Self {
        Self {
            display_mode: BatteryDisplayMode::PercentAndCharging,
            low_battery_threshold: 20.0,
            charging_indicator_style: "text".to_owned(),
        }
    }
}

fn infobar_text_width(font: &FontRef, scale: PxScale, text: &str) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        width += prev.map_or(0.0, |p| scaled.kern(p, id));
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

fn infobar_truncate(font: &FontRef, scale: PxScale, text: &str, max_px: f32) -> String {
    if infobar_text_width(font, scale, text) <= max_px {
        return text.to_owned();
    }
    let ellipsis = "…";
    let available = (max_px - infobar_text_width(font, scale, ellipsis)).max(0.0);
    let scaled = font.as_scaled(scale);
    let mut result = String::new();
    let mut used = 0.0f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        let kern = prev.map_or(0.0, |p| scaled.kern(p, id));
        let advance = scaled.h_advance(id);
        if used + kern + advance > available {
            break;
        }
        used += kern + advance;
        result.push(c);
        prev = Some(id);
    }
    result.push_str(ellipsis);
    result
}

fn draw_progress_bar(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    percent: f32,
    fg: Rgba<u8>,
    bg: Rgba<u8>,
    rounded: bool,
) {
    if width == 0 || height == 0 {
        return;
    }

    draw_filled_rect_mut(img, Rect::at(x, y).of_size(width, height), bg);
    let fill_w = ((width as f32) * (percent.clamp(0.0, 100.0) / 100.0))
        .round()
        .clamp(0.0, width as f32) as u32;
    if fill_w == 0 {
        return;
    }

    if rounded && height >= 4 {
        let radius = (height as i32 / 2).max(1);
        draw_filled_rect_mut(
            img,
            Rect::at(x + radius, y).of_size(fill_w.saturating_sub((radius as u32) * 2), height),
            fg,
        );
        draw_filled_circle_mut(img, (x + radius, y + radius), radius, fg);
        draw_filled_circle_mut(img, (x + (fill_w as i32) - radius - 1, y + radius), radius, fg);
    } else {
        draw_filled_rect_mut(img, Rect::at(x, y).of_size(fill_w, height), fg);
    }
}

pub fn generate_infobar_image(
    info: &NowPlaying,
    artwork_b64: Option<String>,
    show_time: bool,
    options: &NowPlayingRenderOptions,
) -> Option<String> {
    let font = FontRef::try_from_slice(FONT_DATA).ok()?;

    let mut img = RgbaImage::new(INFOBAR_W, INFOBAR_H);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([20, 20, 20, 255]);
    }

    if info.state == "stopped" || info.title.is_empty() {
        let scale = PxScale { x: options.title_font_size, y: options.title_font_size };
        draw_text_mut(
            &mut img,
            Rgba([120, 120, 120, 255]),
            options.left_padding.max(0),
            18,
            scale,
            &font,
            "Not Playing",
        );
    } else {
        let text_x: i32 = if options.show_cover { if let Some(art_data) = artwork_b64 {
            let raw: &str = art_data
                .split_once(',')
                .map(|(_, b)| b)
                .unwrap_or(art_data.as_str());
            match B64.decode(raw) {
                Ok(bytes) => match image::load_from_memory(&bytes) {
                    Ok(dyn_img) => {
                        let thumb = dyn_img
                            .resize_exact(50, 50, image::imageops::FilterType::Lanczos3)
                            .into_rgba8();
                        image::imageops::overlay(&mut img, &thumb, 4, 4);
                        60
                    }
                    Err(_) => options.left_padding.max(0),
                },
                Err(_) => options.left_padding.max(0),
            }
        } else { options.left_padding.max(0) } } else { options.left_padding.max(0) };

        let avail_w = ((INFOBAR_W as i32) - text_x - options.right_padding.max(0)).max(30) as f32;
        let title_scale = PxScale { x: options.title_font_size, y: options.title_font_size };
        let title = infobar_truncate(&font, title_scale, info.title.as_str(), avail_w);
        draw_text_mut(
            &mut img,
            Rgba([255, 255, 255, 255]),
            text_x,
            5,
            title_scale,
            &font,
            &title,
        );

        if options.line_clamp > 1 && !info.artist.is_empty() {
            let artist_scale = PxScale { x: options.artist_font_size, y: options.artist_font_size };
            let artist = infobar_truncate(&font, artist_scale, info.artist.as_str(), avail_w);
            draw_text_mut(
                &mut img,
                Rgba([170, 170, 170, 255]),
                text_x,
                33,
                artist_scale,
                &font,
                &artist,
            );
        }
    }

    if show_time {
        let time_text = Local::now().format("%H:%M").to_string();
        let time_scale = PxScale { x: 14.0, y: 14.0 };
        let padding_x = 5i32;
        let text_w = infobar_text_width(&font, time_scale, &time_text).ceil() as i32;
        let box_w = text_w + (padding_x * 2);
        let box_h = 16i32;
        let box_x = ((INFOBAR_W as i32) - box_w - 4).max(0);
        let box_y = (INFOBAR_H as i32) - box_h - 4;

        draw_filled_rect_mut(
            &mut img,
            Rect::at(box_x, box_y).of_size(box_w as u32, box_h as u32),
            Rgba([0, 0, 0, 185]),
        );
        draw_text_mut(
            &mut img,
            Rgba([255, 255, 255, 255]),
            box_x + padding_x,
            box_y + 1,
            time_scale,
            &font,
            &time_text,
        );
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
}

fn format_remaining(seconds: f32) -> String {
    let clamped = seconds.max(0.0) as i64;
    let mins = clamped / 60;
    let secs = clamped % 60;
    format!("-{}:{:02}", mins, secs)
}

pub fn generate_infobar_nowplaying_progress_image(
    info: &NowPlaying,
    artwork_b64: Option<String>,
    progress: Option<PlaybackProgress>,
    options: &NowPlayingProgressOptions,
) -> Option<String> {
    let font = FontRef::try_from_slice(FONT_DATA).ok()?;
    let mut img = RgbaImage::new(INFOBAR_W, INFOBAR_H);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([20, 20, 20, 255]);
    }

    if (options.fallback_mode == "blank") || (info.state == "stopped" || info.title.is_empty()) {
        if options.fallback_mode == "blank" {
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
            return Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())));
        }
        let scale = PxScale { x: options.base.title_font_size.max(16.0), y: options.base.title_font_size.max(16.0) };
        draw_text_mut(
            &mut img,
            Rgba([120, 120, 120, 255]),
            options.base.left_padding.max(0),
            18,
            scale,
            &font,
            "Not Playing",
        );
    } else {
        let text_x: i32 = if options.base.show_cover { if let Some(art_data) = artwork_b64 {
            let raw: &str = art_data
                .split_once(',')
                .map(|(_, b)| b)
                .unwrap_or(art_data.as_str());
            match B64.decode(raw) {
                Ok(bytes) => match image::load_from_memory(&bytes) {
                    Ok(dyn_img) => {
                        let thumb = dyn_img
                            .resize_exact(50, 50, image::imageops::FilterType::Lanczos3)
                            .into_rgba8();
                        image::imageops::overlay(&mut img, &thumb, 4, 4);
                        60
                    }
                    Err(_) => options.base.left_padding.max(0),
                },
                Err(_) => options.base.left_padding.max(0),
            }
        } else { options.base.left_padding.max(0) } } else { options.base.left_padding.max(0) };

        let avail_w = ((INFOBAR_W as i32) - text_x - options.base.right_padding.max(0)).max(30) as f32;

        draw_text_mut(
            &mut img,
            Rgba([255, 255, 255, 255]),
            text_x,
            3,
            PxScale { x: options.base.title_font_size.min(20.0), y: options.base.title_font_size.min(20.0) },
            &font,
            &infobar_truncate(
                &font,
                PxScale { x: options.base.title_font_size.min(20.0), y: options.base.title_font_size.min(20.0) },
                info.title.as_str(),
                avail_w,
            ),
        );

        if options.base.line_clamp > 1 && !info.artist.is_empty() {
            draw_text_mut(
                &mut img,
                Rgba([170, 170, 170, 255]),
                text_x,
                22,
                PxScale { x: options.base.artist_font_size.min(16.0), y: options.base.artist_font_size.min(16.0) },
                &font,
                &infobar_truncate(
                    &font,
                    PxScale { x: options.base.artist_font_size.min(16.0), y: options.base.artist_font_size.min(16.0) },
                    info.artist.as_str(),
                    avail_w,
                ),
            );
        }

        let remaining = progress.as_ref().map(|p| match options.time_format {
            ProgressTimeFormat::Remaining => format_remaining(p.remaining_secs),
            ProgressTimeFormat::ElapsedTotal => {
                let e = p.elapsed_secs.max(0.0) as i64;
                let d = p.duration_secs.max(0.0) as i64;
                format!("{}:{:02}/{:02}:{:02}", e / 60, e % 60, d / 60, d % 60)
            }
        }).unwrap_or_else(|| "--:--".to_owned());
        let remaining_scale = PxScale { x: 13.0, y: 13.0 };
        let remaining_w = infobar_text_width(&font, remaining_scale, &remaining).ceil() as i32 + options.base.right_padding.max(0);
        let remaining_x = ((INFOBAR_W as i32) - remaining_w).max(text_x + 40);
        draw_text_mut(
            &mut img,
            Rgba([220, 220, 220, 255]),
            remaining_x,
            42,
            remaining_scale,
            &font,
            &remaining,
        );

        let bar_x = text_x;
        let bar_y = 48;
        let bar_h = options.base.progress_bar_height.max(3).min(12);
        let bar_w = ((remaining_x - bar_x - 6).max(20)) as u32;
        let (fg, bg) = if options.base.high_contrast {
            (Rgba([255, 255, 255, 255]), Rgba([25, 25, 25, 255]))
        } else {
            (Rgba([220, 220, 220, 255]), Rgba([55, 55, 55, 255]))
        };

        let pct = progress
            .as_ref()
            .map(|p| p.progress_percent.clamp(0.0, 100.0))
            .unwrap_or(0.0);
        draw_progress_bar(
            &mut img,
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            pct,
            fg,
            bg,
            options.base.rounded_bar,
        );
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
}

pub fn generate_infobar_battery_image(
    status: Option<BatteryStatus>,
    options: &BatteryRenderOptions,
) -> Option<String> {
    let font = FontRef::try_from_slice(FONT_DATA).ok()?;
    let mut img = RgbaImage::new(INFOBAR_W, INFOBAR_H);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([20, 20, 20, 255]);
    }

    draw_text_mut(
        &mut img,
        Rgba([220, 220, 220, 255]),
        8,
        4,
        PxScale { x: 14.0, y: 14.0 },
        &font,
        "Battery",
    );

    match status {
        Some(status) => {
            let pct = status.percent.clamp(0.0, 100.0);
            let pct_text = format!("{}%", pct.round() as i32);
            let low = pct <= options.low_battery_threshold;
            let pct_color = if low {
                Rgba([255, 120, 120, 255])
            } else {
                Rgba([255, 255, 255, 255])
            };
            draw_text_mut(
                &mut img,
                pct_color,
                8,
                18,
                PxScale { x: 24.0, y: 24.0 },
                &font,
                &pct_text,
            );

            match options.display_mode {
                BatteryDisplayMode::PercentOnly => {}
                BatteryDisplayMode::PercentAndCharging | BatteryDisplayMode::IconAndPercent => {
                    let status_text = if status.is_charging {
                        if options.charging_indicator_style == "symbol" { "⚡" } else { "Charging" }
                    } else if options.charging_indicator_style == "symbol" {
                        "🔋"
                    } else {
                        "On Battery"
                    };
                    draw_text_mut(
                        &mut img,
                        Rgba([170, 170, 170, 255]),
                        95,
                        24,
                        PxScale { x: 14.0, y: 14.0 },
                        &font,
                        status_text,
                    );
                }
            }

            let bar_x = 8;
            let bar_y = 46;
            let bar_w = INFOBAR_W - 16;
            let bar_h = 7;
            draw_progress_bar(
                &mut img,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                pct,
                if low { Rgba([255, 140, 140, 255]) } else { Rgba([255, 255, 255, 255]) },
                Rgba([55, 55, 55, 255]),
                false,
            );
        }
        None => {
            draw_text_mut(
                &mut img,
                Rgba([120, 120, 120, 255]),
                8,
                24,
                PxScale { x: 18.0, y: 18.0 },
                &font,
                "Unavailable",
            );
        }
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
}