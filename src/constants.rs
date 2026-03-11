pub const IMG_PLAY: &[u8] =
    include_bytes!("../com.niklasarnitz.macos-utilities.sdPlugin/imgs/play.png");
pub const IMG_PAUSE: &[u8] =
    include_bytes!("../com.niklasarnitz.macos-utilities.sdPlugin/imgs/pause.png");
pub const IMG_MUTE_OFF: &[u8] =
    include_bytes!("../com.niklasarnitz.macos-utilities.sdPlugin/imgs/mute-off.png");
pub const IMG_MUTE_ON: &[u8] =
    include_bytes!("../com.niklasarnitz.macos-utilities.sdPlugin/imgs/mute-on.png");

pub const ACTION_PREVIOUS: &str = "com.niklasarnitz.macos-utilities.previous";
pub const ACTION_NEXT: &str = "com.niklasarnitz.macos-utilities.next";
pub const ACTION_PLAYPAUSE: &str = "com.niklasarnitz.macos-utilities.playpause";
pub const ACTION_VOLUME_UP: &str = "com.niklasarnitz.macos-utilities.volume-up";
pub const ACTION_VOLUME_DOWN: &str = "com.niklasarnitz.macos-utilities.volume-down";
pub const ACTION_MUTE: &str = "com.niklasarnitz.macos-utilities.mute";
pub const ACTION_INFOBAR_NOWPLAYING: &str =
    "com.niklasarnitz.macos-utilities.infobar.nowplaying";
pub const ACTION_INFOBAR_NOWPLAYING_TIME: &str =
    "com.niklasarnitz.macos-utilities.infobar.nowplaying-time";
pub const ACTION_INFOBAR_NOWPLAYING_PROGRESS: &str =
    "com.niklasarnitz.macos-utilities.infobar.nowplaying-progress";
pub const ACTION_INFOBAR_NEXT_CALENDAR: &str =
    "com.niklasarnitz.macos-utilities.infobar.nextcalendar";
pub const ACTION_INFOBAR_BATTERY: &str = "com.niklasarnitz.macos-utilities.infobar.battery";

pub const INFOBAR_W: u32 = 248;
pub const INFOBAR_H: u32 = 58;
pub const FONT_DATA: &[u8] = include_bytes!(
    "../com.niklasarnitz.macos-utilities.sdPlugin/fonts/Roboto-Regular.ttf"
);