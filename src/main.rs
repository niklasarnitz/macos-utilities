#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc2 as objc;

mod calendar;
mod constants;
mod infobar;
mod protocol;
mod system_media;

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use constants::{
    ACTION_INFOBAR_BATTERY, ACTION_INFOBAR_NEXT_CALENDAR,
    ACTION_INFOBAR_NOWPLAYING, ACTION_INFOBAR_NOWPLAYING_PROGRESS, ACTION_INFOBAR_NOWPLAYING_TIME,
    ACTION_MUTE, ACTION_NEXT, ACTION_PLAYPAUSE, ACTION_PREVIOUS,
    ACTION_VOLUME_DOWN, ACTION_VOLUME_UP,
};
use futures_util::{SinkExt, StreamExt};
use infobar::{
    generate_infobar_battery_image, generate_infobar_image, generate_infobar_nowplaying_progress_image,
    BatteryDisplayMode, BatteryRenderOptions, NowPlayingProgressOptions, NowPlayingRenderOptions,
    ProgressTimeFormat,
};
use protocol::{
    make_set_infobar_item_visibility, GetSettings, IncomingMessage,
    InfobarComponent, RegisterEvent, SetImage, SetImagePayload, SetInfobarComponent,
    SetInfobarComponentPayload, SetState, SetStatePayload, ShowInfobarPopover,
    ShowInfobarPopoverPayload,
};
use system_media::{
    adjust_volume, current_volume_percent, get_artwork_b64, get_battery_status, get_now_playing,
    get_now_playing_progress, is_media_playing, is_muted, press_next, press_play_pause,
    press_previous, toggle_mute,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type ActiveContexts = Arc<Mutex<HashMap<String, String>>>;
type VolumeSteps = Arc<Mutex<HashMap<String, i32>>>;
type SettingsStore = Arc<Mutex<HashMap<String, serde_json::Value>>>;
type ProgressSmoothingStore = Arc<Mutex<HashMap<String, f32>>>;

fn settings_i64(settings: Option<&serde_json::Value>, key: &str, default: i64) -> i64 {
    settings
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
}

fn settings_f32(settings: Option<&serde_json::Value>, key: &str, default: f32) -> f32 {
    settings
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn settings_bool(settings: Option<&serde_json::Value>, key: &str, default: bool) -> bool {
    settings
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn settings_string(settings: Option<&serde_json::Value>, key: &str, default: &str) -> String {
    settings
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .map(|v| v.to_owned())
        .unwrap_or_else(|| default.to_owned())
}

fn nowplaying_render_options(settings: Option<&serde_json::Value>) -> NowPlayingRenderOptions {
    NowPlayingRenderOptions {
        show_cover: settings_bool(settings, "showCover", true),
        title_font_size: settings_f32(settings, "titleFontSize", 22.0).clamp(12.0, 30.0),
        artist_font_size: settings_f32(settings, "artistFontSize", 18.0).clamp(10.0, 24.0),
        line_clamp: settings_i64(settings, "lineClamp", 2).clamp(1, 2) as u8,
        left_padding: settings_i64(settings, "leftPadding", 10).clamp(0, 30) as i32,
        right_padding: settings_i64(settings, "rightPadding", 6).clamp(0, 30) as i32,
        progress_bar_height: settings_i64(settings, "progressBarHeight", 6).clamp(3, 12) as u32,
        high_contrast: settings_bool(settings, "highContrast", false),
        rounded_bar: settings_bool(settings, "roundedBar", false),
    }
}

fn nowplaying_progress_options(settings: Option<&serde_json::Value>) -> NowPlayingProgressOptions {
    let time_format = if settings_string(settings, "timeFormat", "remaining") == "elapsed_total" {
        ProgressTimeFormat::ElapsedTotal
    } else {
        ProgressTimeFormat::Remaining
    };

    NowPlayingProgressOptions {
        base: nowplaying_render_options(settings),
        time_format,
        fallback_mode: settings_string(settings, "fallbackMode", "no_track"),
    }
}

fn battery_render_options(settings: Option<&serde_json::Value>) -> BatteryRenderOptions {
    let display_mode = match settings_string(settings, "batteryDisplayMode", "percent_and_charging").as_str() {
        "percent_only" => BatteryDisplayMode::PercentOnly,
        "icon_and_percent" => BatteryDisplayMode::IconAndPercent,
        _ => BatteryDisplayMode::PercentAndCharging,
    };

    BatteryRenderOptions {
        display_mode,
        low_battery_threshold: settings_f32(settings, "lowBatteryThreshold", 20.0).clamp(5.0, 50.0),
        charging_indicator_style: settings_string(settings, "chargingIndicatorStyle", "text"),
    }
}

fn init_logger() {
    use simplelog::*;
    if let Err(error) = TermLogger::init(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Stdout,
        ColorChoice::Never,
    ) {
        eprintln!("Logger initialization failed: {}", error);
    }
}

fn is_volume_step_action(action: &str) -> bool {
    matches!(action, ACTION_VOLUME_UP | ACTION_VOLUME_DOWN)
}

fn is_active_sync_action(action: &str) -> bool {
    matches!(
        action,
        ACTION_PLAYPAUSE
            | ACTION_MUTE
            | ACTION_INFOBAR_NOWPLAYING
            | ACTION_INFOBAR_NOWPLAYING_PROGRESS
            | ACTION_INFOBAR_NOWPLAYING_TIME
            | ACTION_INFOBAR_NEXT_CALENDAR
            | ACTION_INFOBAR_BATTERY
    )
}

struct ParsedActionContext {
    device: String,
    controller: String,
    position: u8,
}

fn parse_action_context(context: &str) -> Option<ParsedActionContext> {
    let segments: Vec<&str> = context.split('.').collect();
    if segments.len() < 5 {
        return None;
    }
    Some(ParsedActionContext {
        device: segments[0].to_owned(),
        controller: segments[2].to_owned(),
        position: segments[3].parse().ok()?,
    })
}

fn send_volume_infobar_overlay(source_context: &str, active: &ActiveContexts, tx: &UnboundedSender<String>) {
    let Some(source) = parse_action_context(source_context) else {
        return;
    };
    let Some(volume) = current_volume_percent() else {
        return;
    };

    let targets: Vec<String> = active
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(ctx, action)| {
            let parsed = parse_action_context(ctx)?;
            (parsed.controller == "Infobar"
                && parsed.device == source.device)
                .then_some(ctx.clone())
        })
        .collect();

    for target_context in targets {
        let payload = ShowInfobarPopover {
            event: "showInfobarPopover",
            payload: ShowInfobarPopoverPayload {
                context: target_context,
                priority: 220,
                duration_ms: 1200,
                component: InfobarComponent::ProgressBar {
                    label: "Volume".to_owned(),
                    value: volume.round(),
                    min: 0.0,
                    max: 100.0,
                },
            },
        };
        let _ = tx.send(serde_json::to_string(&payload).unwrap());
    }
}

fn build_sync_messages(
    ctx: &str,
    action: &str,
    settings_store: &SettingsStore,
    progress_smoothing: &ProgressSmoothingStore,
) -> Vec<String> {
    let settings = settings_store.lock().unwrap().get(ctx).cloned();

    match action {
        ACTION_INFOBAR_NOWPLAYING => {
            let info = get_now_playing();
            let is_displaying_data = info.state == "playing" && !info.title.is_empty();
            let options = nowplaying_render_options(settings.as_ref());
            let artwork = if options.show_cover { get_artwork_b64() } else { None };
            log::debug!("[sync] infobar.nowplaying → {} / {}", info.title, info.artist);
            let mut messages = vec![make_set_infobar_item_visibility(ctx, is_displaying_data)];
            if !is_displaying_data {
                return messages;
            }

            if let Some(image) = artwork.clone() {
                messages.push(
                    serde_json::to_string(&SetInfobarComponent {
                        event: "setInfobarComponent",
                        payload: SetInfobarComponentPayload {
                            context: ctx.to_owned(),
                            component: InfobarComponent::ImageTitleSubtitle {
                                image,
                                title: info.title.clone(),
                                subtitle: info.artist.clone(),
                            },
                        },
                    })
                    .unwrap(),
                );
            } else if let Some(data_uri) = generate_infobar_image(&info, artwork, false, &options) {
                messages.push(
                    serde_json::to_string(&SetImage {
                        event: "setImage",
                        context: ctx.to_owned(),
                        payload: SetImagePayload {
                            image: data_uri,
                            target: 0,
                        },
                    })
                    .unwrap(),
                );
            }
            messages
        }
        ACTION_INFOBAR_NOWPLAYING_TIME => {
            let info = get_now_playing();
            let is_displaying_data = info.state == "playing" && !info.title.is_empty();
            let options = nowplaying_render_options(settings.as_ref());
            let artwork = if options.show_cover { get_artwork_b64() } else { None };
            log::debug!(
                "[sync] infobar.nowplaying-time → {} / {}",
                info.title, info.artist
            );
            let mut messages = vec![make_set_infobar_item_visibility(ctx, is_displaying_data)];
            if !is_displaying_data {
                return messages;
            }

            if let Some(data_uri) = generate_infobar_image(&info, artwork, true, &options) {
                messages.push(
                    serde_json::to_string(&SetImage {
                        event: "setImage",
                        context: ctx.to_owned(),
                        payload: SetImagePayload {
                            image: data_uri,
                            target: 0,
                        },
                    })
                    .unwrap(),
                );
            }
            messages
        }
        ACTION_INFOBAR_NOWPLAYING_PROGRESS => {
            let info = get_now_playing();
            let is_displaying_data = info.state == "playing" && !info.title.is_empty();
            let mut messages = vec![make_set_infobar_item_visibility(ctx, is_displaying_data)];
            if !is_displaying_data {
                return messages;
            }

            let options = nowplaying_progress_options(settings.as_ref());
            if options.fallback_mode == "keep_last" && (info.state == "stopped" || info.title.is_empty()) {
                return messages;
            }

            let artwork = if options.base.show_cover { get_artwork_b64() } else { None };
            let mut progress = get_now_playing_progress();
            if settings_bool(settings.as_ref(), "progressSmoothing", false) {
                if let Some(current) = progress.as_ref().map(|p| p.progress_percent) {
                    let mut smoothing = progress_smoothing.lock().unwrap();
                    let smoothed = if let Some(prev) = smoothing.get(ctx).copied() {
                        (prev * 0.7) + (current * 0.3)
                    } else {
                        current
                    };
                    smoothing.insert(ctx.to_owned(), smoothed);
                    if let Some(progress_ref) = progress.as_mut() {
                        progress_ref.progress_percent = smoothed.clamp(0.0, 100.0);
                    }
                }
            }
            log::debug!(
                "[sync] infobar.nowplaying-progress → {} / {}",
                info.title, info.artist
            );
            if let Some(data_uri) =
                generate_infobar_nowplaying_progress_image(&info, artwork, progress, &options)
            {
                messages.push(
                    serde_json::to_string(&SetImage {
                        event: "setImage",
                        context: ctx.to_owned(),
                        payload: SetImagePayload {
                            image: data_uri,
                            target: 0,
                        },
                    })
                    .unwrap(),
                );
            }
            messages
        }
        ACTION_INFOBAR_NEXT_CALENDAR => {
            let selected_calendars = settings_string(settings.as_ref(), "calendarNames", "")
                .split(',')
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_owned())
                .collect::<Vec<_>>();

            let event_result = calendar::get_next_event(&selected_calendars);
            let visible = matches!(event_result, Ok(Some(_)));
            let mut messages = vec![make_set_infobar_item_visibility(ctx, visible)];

            let image = match &event_result {
                Ok(Some(event)) => calendar::render_next_calendar_image(Some(event), None),
                Ok(None) => calendar::render_next_calendar_image(None, None),
                Err(error) => calendar::render_next_calendar_image(None, Some(error)),
            };

            if let Some(data_uri) = image {
                messages.push(
                    serde_json::to_string(&SetImage {
                        event: "setImage",
                        context: ctx.to_owned(),
                        payload: SetImagePayload {
                            image: data_uri,
                            target: 0,
                        },
                    })
                    .unwrap(),
                );
            }

            messages
        }
        ACTION_INFOBAR_BATTERY => {
            let battery = get_battery_status();
            let options = battery_render_options(settings.as_ref());
            if let Some(data_uri) = generate_infobar_battery_image(battery, &options) {
                vec![serde_json::to_string(&SetImage {
                    event: "setImage",
                    context: ctx.to_owned(),
                    payload: SetImagePayload {
                        image: data_uri,
                        target: 0,
                    },
                })
                .unwrap()]
            } else {
                vec![]
            }
        }
        ACTION_PLAYPAUSE => {
            let playing = is_media_playing();
            let state = if playing { 1u32 } else { 0 };
            log::debug!("[sync] playpause → state={state} (playing={playing})");
            vec![serde_json::to_string(&SetState {
                event: "setState",
                context: ctx,
                payload: SetStatePayload { state },
            })
            .unwrap()]
        }
        ACTION_MUTE => {
            let muted = is_muted();
            let state = if muted { 1u32 } else { 0 };
            log::debug!("[sync] mute → state={state} (muted={muted})");
            vec![serde_json::to_string(&SetState {
                event: "setState",
                context: ctx,
                payload: SetStatePayload { state },
            })
            .unwrap()]
        }
        _ => vec![],
    }
}

fn send_sync(
    ctx: &str,
    action: &str,
    settings_store: &SettingsStore,
    progress_smoothing: &ProgressSmoothingStore,
    tx: &UnboundedSender<String>,
) {
    for msg in build_sync_messages(ctx, action, settings_store, progress_smoothing) {
        let _ = tx.send(msg);
    }
}

fn send_sync_all(
    active: &ActiveContexts,
    settings_store: &SettingsStore,
    progress_smoothing: &ProgressSmoothingStore,
    tx: &UnboundedSender<String>,
) {
    let contexts: Vec<(String, String)> = active
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (ctx, action) in contexts {
        send_sync(&ctx, &action, settings_store, progress_smoothing, tx);
    }
}

fn handle_key_down(
    action: &str,
    context: &str,
    tx: &UnboundedSender<String>,
    steps: &VolumeSteps,
    active: &ActiveContexts,
    settings_store: &SettingsStore,
    progress_smoothing: &ProgressSmoothingStore,
) {
    match action {
        ACTION_PREVIOUS => {
            log::debug!("[keyDown] previous track");
            press_previous();
        }
        ACTION_NEXT => {
            log::debug!("[keyDown] next track");
            press_next();
        }
        ACTION_PLAYPAUSE => {
            log::debug!("[keyDown] play/pause");
            press_play_pause();
        }
        ACTION_VOLUME_UP => {
            let step = steps.lock().unwrap().get(context).copied().unwrap_or(10);
            log::debug!("[keyDown] volume up +{step}");
            adjust_volume(step);
            send_volume_infobar_overlay(context, active, tx);
        }
        ACTION_VOLUME_DOWN => {
            let step = steps.lock().unwrap().get(context).copied().unwrap_or(10);
            log::debug!("[keyDown] volume down -{step}");
            adjust_volume(-step);
            send_volume_infobar_overlay(context, active, tx);
        }
        ACTION_MUTE => {
            log::debug!("[keyDown] mute toggle");
            toggle_mute();
        }
        _ => {
            log::warn!("[keyDown] unknown action: {action}");
        }
    }
}

fn handle_message(
    msg: IncomingMessage,
    active: &ActiveContexts,
    steps: &VolumeSteps,
    settings_store: &SettingsStore,
    progress_smoothing: &ProgressSmoothingStore,
    tx: &UnboundedSender<String>,
) {
    let action = msg.action.unwrap_or_default();
    let context = msg.context.unwrap_or_default();

    match msg.event.as_str() {
        "willAppear" => {
            log::debug!("[willAppear] {action}");

            if is_volume_step_action(&action) || is_active_sync_action(&action) {
                let _ = tx.send(
                    serde_json::to_string(&GetSettings {
                        event: "getSettings",
                        context: &context,
                    })
                    .unwrap(),
                );
            }

            if is_active_sync_action(&action) {
                active
                    .lock()
                    .unwrap()
                    .insert(context.clone(), action.clone());

                if action != ACTION_PLAYPAUSE {
                    send_sync(&context, &action, settings_store, progress_smoothing, tx);
                }
            }
        }
        "willDisappear" => {
            log::debug!("[willDisappear] {action}");
            active.lock().unwrap().remove(&context);
            steps.lock().unwrap().remove(&context);
            settings_store.lock().unwrap().remove(&context);
            progress_smoothing.lock().unwrap().remove(&context);
        }
        "didReceiveSettings" => {
            let settings = msg
                .payload
                .as_ref()
                .and_then(|p| p.get("settings"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let step = settings
                .get("step")
                .and_then(|v| v.as_i64())
                .map(|v| v.clamp(1, 50) as i32)
                .unwrap_or(10);
            log::debug!("[settings] {action} step={step}");
            steps.lock().unwrap().insert(context.clone(), step);
            settings_store
                .lock()
                .unwrap()
                .insert(context, settings);
        }
        "keyDown" => {
            handle_key_down(
                &action,
                &context,
                tx,
                steps,
                active,
                settings_store,
                progress_smoothing,
            );

            let active = Arc::clone(active);
            let settings_store = Arc::clone(settings_store);
            let progress_smoothing = Arc::clone(progress_smoothing);
            let tx = tx.clone();
            tokio::spawn(async move {
                time::sleep(Duration::from_millis(50)).await;
                send_sync_all(&active, &settings_store, &progress_smoothing, &tx);
            });
        }
        other => {
            log::debug!("[event] {other}");
        }
    }
}

#[tokio::main]
async fn main() {
    init_logger();

    let argv: Vec<String> = env::args().collect();
    let mut port = String::new();
    let mut plugin_uuid = String::new();
    let mut register_event = String::new();

    let mut i = 1;
    while i < argv.len() {
        match argv[i].trim_start_matches('-') {
            "port" if i + 1 < argv.len() => {
                port = argv[i + 1].clone();
                i += 2;
            }
            "pluginUUID" if i + 1 < argv.len() => {
                plugin_uuid = argv[i + 1].clone();
                i += 2;
            }
            "registerEvent" if i + 1 < argv.len() => {
                register_event = argv[i + 1].clone();
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    if port.is_empty() || plugin_uuid.is_empty() || register_event.is_empty() {
        log::error!(
            "Usage: macos-utilities \\
             -port <port> -pluginUUID <uuid> -registerEvent <event> -info <json>"
        );
        std::process::exit(1);
    }

    let url = format!("ws://localhost:{}", port);
    log::info!("[startup] connecting to {url} as {plugin_uuid}");
    let (ws_stream, _) = connect_async(&url)
        .await
        .expect("WebSocket connection failed");
    log::info!("[startup] connected, registering plugin");
    let (mut writer, mut reader) = ws_stream.split();

    let reg = serde_json::to_string(&RegisterEvent {
        event: &register_event,
        uuid: &plugin_uuid,
    })
    .unwrap();
    writer
        .send(Message::Text(reg))
        .await
        .expect("registration send failed");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if writer.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let active: ActiveContexts = Arc::new(Mutex::new(HashMap::new()));
    let steps: VolumeSteps = Arc::new(Mutex::new(HashMap::new()));
    let settings_store: SettingsStore = Arc::new(Mutex::new(HashMap::new()));
    let progress_smoothing: ProgressSmoothingStore = Arc::new(Mutex::new(HashMap::new()));

    {
        let active = Arc::clone(&active);
        let settings_store = Arc::clone(&settings_store);
        let progress_smoothing = Arc::clone(&progress_smoothing);
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(100));
            let mut last_sent: HashMap<String, String> = HashMap::new();
            let mut last_tick: HashMap<String, Instant> = HashMap::new();
            loop {
                interval.tick().await;
                let contexts: Vec<(String, String)> = active
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                let mut present_contexts: HashMap<String, ()> = HashMap::new();
                for (ctx, action) in contexts {
                    present_contexts.insert(ctx.clone(), ());
                    let settings = settings_store.lock().unwrap().get(&ctx).cloned();
                    let update_interval_ms = settings_i64(settings.as_ref(), "updateIntervalMs", 250)
                        .clamp(100, 5000) as u64;
                    let should_tick = last_tick
                        .get(&ctx)
                        .map(|last| last.elapsed() >= Duration::from_millis(update_interval_ms))
                        .unwrap_or(true);
                    if !should_tick {
                        continue;
                    }

                    let messages =
                        build_sync_messages(&ctx, &action, &settings_store, &progress_smoothing);
                    let fingerprint = messages.join("\u{1f}");
                    let should_send = last_sent.get(&ctx) != Some(&fingerprint);
                    if should_send {
                        for msg in messages {
                            let _ = tx.send(msg);
                        }
                        last_sent.insert(ctx.clone(), fingerprint);
                    }
                    last_tick.insert(ctx, Instant::now());
                }

                last_sent.retain(|ctx, _| present_contexts.contains_key(ctx));
                last_tick.retain(|ctx, _| present_contexts.contains_key(ctx));
            }
        });
    }

    while let Some(Ok(msg)) = reader.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<IncomingMessage>(&text) {
                Ok(m) => handle_message(
                    m,
                    &active,
                    &steps,
                    &settings_store,
                    &progress_smoothing,
                    &tx,
                ),
                Err(e) => log::warn!("JSON parse error: {e}"),
            }
        }
    }
}
