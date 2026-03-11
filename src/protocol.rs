use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct IncomingMessage {
    pub event: String,
    pub action: Option<String>,
    pub context: Option<String>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct RegisterEvent<'a> {
    pub event: &'a str,
    pub uuid: &'a str,
}

#[derive(Serialize)]
pub struct GetSettings<'a> {
    pub event: &'a str,
    pub context: &'a str,
}

#[derive(Serialize)]
pub struct SetState<'a> {
    pub event: &'a str,
    pub context: &'a str,
    pub payload: SetStatePayload,
}

#[derive(Serialize)]
pub struct SetStatePayload {
    pub state: u32,
}

#[derive(Serialize)]
pub struct SetTitle<'a> {
    pub event: &'a str,
    pub context: &'a str,
    pub payload: SetTitlePayload<'a>,
}

#[derive(Serialize)]
pub struct SetTitlePayload<'a> {
    pub title: &'a str,
    pub target: u32,
}

#[derive(Serialize)]
pub struct SetImage {
    pub event: &'static str,
    pub context: String,
    pub payload: SetImagePayload,
}

#[derive(Serialize)]
pub struct SetImagePayload {
    pub image: String,
    pub target: u32,
}

#[derive(Serialize)]
pub struct ShowInfobarPopover {
    pub event: &'static str,
    pub payload: ShowInfobarPopoverPayload,
}

#[derive(Serialize)]
pub struct SetInfobarComponent {
    pub event: &'static str,
    pub payload: SetInfobarComponentPayload,
}

#[derive(Serialize)]
pub struct SetInfobarItemVisibility<'a> {
    pub event: &'static str,
    pub context: &'a str,
    pub payload: SetInfobarItemVisibilityPayload,
}

#[derive(Serialize)]
pub struct SetInfobarItemVisibilityPayload {
    pub visible: bool,
}

#[derive(Serialize)]
pub struct SetInfobarComponentPayload {
    pub context: String,
    pub component: InfobarComponent,
}

#[derive(Serialize)]
pub struct ShowInfobarPopoverPayload {
    pub context: String,
    pub priority: u8,
    pub duration_ms: u64,
    pub component: InfobarComponent,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InfobarComponent {
    ImageTitleSubtitle {
        image: String,
        title: String,
        subtitle: String,
    },
    ProgressBar {
        label: String,
        value: f32,
        min: f32,
        max: f32,
    },
}

fn png_data_uri(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", B64.encode(bytes))
}

pub fn make_set_image(ctx: &str, bytes: &[u8]) -> String {
    serde_json::to_string(&SetImage {
        event: "setImage",
        context: ctx.to_owned(),
        payload: SetImagePayload {
            image: png_data_uri(bytes),
            target: 0,
        },
    })
    .unwrap()
}

pub fn make_set_infobar_item_visibility(ctx: &str, visible: bool) -> String {
    serde_json::to_string(&SetInfobarItemVisibility {
        event: "setInfobarItemVisibility",
        context: ctx,
        payload: SetInfobarItemVisibilityPayload { visible },
    })
    .unwrap()
}