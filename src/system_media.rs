pub struct NowPlaying {
    pub state: String,
    pub title: String,
    pub artist: String,
}

pub struct PlaybackProgress {
    pub elapsed_secs: f32,
    pub duration_secs: f32,
    pub remaining_secs: f32,
    pub progress_percent: f32,
}

pub struct BatteryStatus {
    pub percent: f32,
    pub is_charging: bool,
}

#[cfg(target_os = "macos")]
mod applescript_fallback {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use std::process::Command;

    fn run(lines: &[&str]) -> String {
        let mut args: Vec<&str> = Vec::with_capacity(lines.len() * 2);
        for line in lines {
            args.push("-e");
            args.push(line);
        }
        Command::new("osascript")
            .args(&args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    }

    pub fn get_now_playing() -> Option<(String, String, String)> {
        let out = run(&[
            "if application \"Music\" is running then",
            "  tell application \"Music\"",
            "    try",
            "      if player state is playing then",
            "        return \"playing|\" & (name of current track) & \"|\" & (artist of current track)",
            "      else if player state is paused then",
            "        return \"paused|\" & (name of current track) & \"|\" & (artist of current track)",
            "      end if",
            "    end try",
            "  end tell",
            "end if",
            "return \"stopped||\"",
        ]);

        let parts: Vec<&str> = out.splitn(3, '|').collect();
        if parts.len() == 3 {
            return Some((
                parts[0].to_owned(),
                parts[1].to_owned(),
                parts[2].to_owned(),
            ));
        }
        None
    }

    pub fn get_artwork_data_uri() -> Option<String> {
        let art_path = "/tmp/macos_media_controls_artwork_fallback";
        let script = format!(
            concat!(
                "set artPath to \"{}\"\n",
                "try\n",
                "  if application \"Music\" is running then\n",
                "    tell application \"Music\"\n",
                "      if player state is playing or player state is paused then\n",
                "        if (count artworks of current track) > 0 then\n",
                "          set artData to raw data of artwork 1 of current track\n",
                "          set fRef to open for access POSIX file artPath with write permission\n",
                "          set eof fRef to 0\n",
                "          write artData to fRef\n",
                "          close access fRef\n",
                "          return \"ok\"\n",
                "        end if\n",
                "      end if\n",
                "    end tell\n",
                "  end if\n",
                "end try\n",
                "return \"fail\""
            ),
            art_path
        );

        let output = Command::new("osascript").args(["-e", &script]).output().ok()?;
        if !String::from_utf8_lossy(&output.stdout)
            .trim()
            .starts_with("ok")
        {
            return None;
        }

        let bytes = std::fs::read(art_path).ok()?;
        let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else {
            "image/png"
        };
        Some(format!("data:{};base64,{}", mime, B64.encode(bytes)))
    }

    pub fn get_now_playing_progress() -> Option<(f32, f32)> {
        let out = run(&[
            "if application \"Music\" is running then",
            "  tell application \"Music\"",
            "    try",
            "      if player state is playing or player state is paused then",
            "        set elapsed to player position",
            "        set total to duration of current track",
            "        return (elapsed as string) & \"|\" & (total as string)",
            "      end if",
            "    end try",
            "  end tell",
            "end if",
            "return \"\"",
        ]);

        let parts: Vec<&str> = out.splitn(2, '|').collect();
        if parts.len() != 2 {
            return None;
        }

        let elapsed = parts[0].trim().replace(',', ".").parse::<f32>().ok()?;
        let duration = parts[1].trim().replace(',', ".").parse::<f32>().ok()?;
        (duration > 0.0).then_some((elapsed.max(0.0), duration))
    }
}

#[cfg(target_os = "macos")]
mod media_keys {
    use objc::{class, msg_send, runtime::Object};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }

    unsafe impl objc::Encode for NSPoint {
        const ENCODING: objc::Encoding =
            objc::Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventPost(tap: u32, event: *const std::ffi::c_void);
    }

    const CG_HID_EVENT_TAP: u32 = 0;
    const NS_EVENT_TYPE_SYSTEM_DEFINED: u64 = 14;
    const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;

    pub const KEY_PLAY: i32 = 16;
    pub const KEY_NEXT: i32 = 17;
    pub const KEY_PREV: i32 = 18;

    pub fn send(key_code: i32) {
        let zero = NSPoint { x: 0.0, y: 0.0 };
        let down_flags: usize = 0x0a00;
        let up_flags: usize = 0x0b00;
        let down_data1: isize = ((key_code as isize) << 16) | 0x0a00;
        let up_data1: isize = ((key_code as isize) << 16) | 0x0b00;
        let null_ctx = std::ptr::null::<Object>();

        unsafe {
            let ev_down: *mut Object = msg_send![
                class!(NSEvent),
                otherEventWithType: NS_EVENT_TYPE_SYSTEM_DEFINED
                location: zero
                modifierFlags: down_flags
                timestamp: 0.0f64
                windowNumber: 0isize
                context: null_ctx
                subtype: NX_SUBTYPE_AUX_CONTROL_BUTTONS
                data1: down_data1
                data2: (-1isize)
            ];
            if !ev_down.is_null() {
                let cg: *const std::ffi::c_void = msg_send![ev_down, CGEvent];
                if !cg.is_null() {
                    CGEventPost(CG_HID_EVENT_TAP, cg);
                }
            }

            let ev_up: *mut Object = msg_send![
                class!(NSEvent),
                otherEventWithType: NS_EVENT_TYPE_SYSTEM_DEFINED
                location: zero
                modifierFlags: up_flags
                timestamp: 0.0f64
                windowNumber: 0isize
                context: null_ctx
                subtype: NX_SUBTYPE_AUX_CONTROL_BUTTONS
                data1: up_data1
                data2: (-1isize)
            ];
            if !ev_up.is_null() {
                let cg: *const std::ffi::c_void = msg_send![ev_up, CGEvent];
                if !cg.is_null() {
                    CGEventPost(CG_HID_EVENT_TAP, cg);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod coreaudio_system {
    use std::ffi::c_void;

    type AudioObjectID = u32;
    type AudioObjectPropertySelector = u32;
    type AudioObjectPropertyScope = u32;
    type AudioObjectPropertyElement = u32;
    type OSStatus = i32;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        mSelector: AudioObjectPropertySelector,
        mScope: AudioObjectPropertyScope,
        mElement: AudioObjectPropertyElement,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectHasProperty(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
        ) -> bool;

        fn AudioObjectGetPropertyData(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
            inQualifierDataSize: u32,
            inQualifierData: *const c_void,
            ioDataSize: *mut u32,
            outData: *mut c_void,
        ) -> OSStatus;

        fn AudioObjectSetPropertyData(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
            inQualifierDataSize: u32,
            inQualifierData: *const c_void,
            inDataSize: u32,
            inData: *const c_void,
        ) -> OSStatus;
    }

    const fn fourcc(bytes: [u8; 4]) -> u32 {
        ((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | (bytes[3] as u32)
    }

    const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = fourcc(*b"glob");
    const K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope = fourcc(*b"outp");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;

    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
        fourcc(*b"dOut");
    const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME:
        AudioObjectPropertySelector = fourcc(*b"vmvc");
    const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_MUTE:
        AudioObjectPropertySelector = fourcc(*b"vmut");
    const K_AUDIO_DEVICE_PROPERTY_MUTE: AudioObjectPropertySelector = fourcc(*b"mute");
    const K_AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR: AudioObjectPropertySelector = fourcc(*b"volm");
    const LEFT_CHANNEL: AudioObjectPropertyElement = 1;
    const RIGHT_CHANNEL: AudioObjectPropertyElement = 2;

    fn has_property(
        object_id: AudioObjectID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
    ) -> bool {
        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        };
        unsafe { AudioObjectHasProperty(object_id, &address) }
    }

    fn default_output_device() -> Option<AudioObjectID> {
        let address = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut device_id: AudioObjectID = 0;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut device_id as *mut AudioObjectID).cast::<c_void>(),
            )
        };

        if status == 0 && device_id != 0 {
            Some(device_id)
        } else {
            None
        }
    }

    fn get_f32_property(
        object_id: AudioObjectID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
    ) -> Option<f32> {
        if !has_property(object_id, selector, scope, element) {
            return None;
        }

        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        };
        let mut value: f32 = 0.0;
        let mut size = std::mem::size_of::<f32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object_id,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut f32).cast::<c_void>(),
            )
        };
        (status == 0).then_some(value)
    }

    fn set_f32_property(
        object_id: AudioObjectID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
        value: f32,
    ) -> bool {
        if !has_property(object_id, selector, scope, element) {
            return false;
        }

        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        };
        let status = unsafe {
            AudioObjectSetPropertyData(
                object_id,
                &address,
                0,
                std::ptr::null(),
                std::mem::size_of::<f32>() as u32,
                (&value as *const f32).cast::<c_void>(),
            )
        };
        status == 0
    }

    fn get_u32_property(
        object_id: AudioObjectID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
    ) -> Option<u32> {
        if !has_property(object_id, selector, scope, element) {
            return None;
        }

        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        };
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object_id,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast::<c_void>(),
            )
        };
        (status == 0).then_some(value)
    }

    fn set_u32_property(
        object_id: AudioObjectID,
        selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope,
        element: AudioObjectPropertyElement,
        value: u32,
    ) -> bool {
        if !has_property(object_id, selector, scope, element) {
            return false;
        }

        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        };
        let status = unsafe {
            AudioObjectSetPropertyData(
                object_id,
                &address,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                (&value as *const u32).cast::<c_void>(),
            )
        };
        status == 0
    }

    pub fn output_volume() -> Option<f32> {
        let device = default_output_device()?;
        if let Some(volume) = get_f32_property(
            device,
            K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        ) {
            return Some(volume.clamp(0.0, 1.0));
        }

        let left = get_f32_property(
            device,
            K_AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            LEFT_CHANNEL,
        );
        let right = get_f32_property(
            device,
            K_AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            RIGHT_CHANNEL,
        );

        match (left, right) {
            (Some(l), Some(r)) => Some(((l + r) / 2.0).clamp(0.0, 1.0)),
            (Some(v), None) | (None, Some(v)) => Some(v.clamp(0.0, 1.0)),
            (None, None) => None,
        }
    }

    pub fn set_output_volume(volume: f32) -> bool {
        let Some(device) = default_output_device() else {
            return false;
        };

        let clamped = volume.clamp(0.0, 1.0);

        if set_f32_property(
            device,
            K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            clamped,
        ) {
            return true;
        }

        let left_ok = set_f32_property(
            device,
            K_AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            LEFT_CHANNEL,
            clamped,
        );
        let right_ok = set_f32_property(
            device,
            K_AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            RIGHT_CHANNEL,
            clamped,
        );

        left_ok || right_ok
    }

    pub fn output_muted() -> Option<bool> {
        let device = default_output_device()?;
        get_u32_property(
            device,
            K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_MUTE,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        )
        .or_else(|| {
            get_u32_property(
                device,
                K_AUDIO_DEVICE_PROPERTY_MUTE,
                K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
                K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            )
        })
        .map(|v| v != 0)
    }

    pub fn set_output_muted(muted: bool) -> bool {
        let Some(device) = default_output_device() else {
            return false;
        };
        let value: u32 = if muted { 1 } else { 0 };

        set_u32_property(
            device,
            K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_MUTE,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            value,
        ) || set_u32_property(
            device,
            K_AUDIO_DEVICE_PROPERTY_MUTE,
            K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
            value,
        )
    }
}

#[cfg(target_os = "macos")]
mod now_playing_macos {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use objc::{class, msg_send, rc::autoreleasepool, runtime::Object};
    use std::ffi::{c_char, CStr};

    #[link(name = "MediaPlayer", kind = "framework")]
    extern "C" {}

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    const NS_UTF8_STRING_ENCODING: usize = 4;
    const NS_BITMAP_IMAGE_FILE_TYPE_PNG: usize = 4;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    unsafe impl objc::Encode for CGSize {
        const ENCODING: objc::Encoding =
            objc::Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    unsafe fn nsstring_from_str(s: &str) -> *mut Object {
        let ns_string: *mut Object = msg_send![class!(NSString), alloc];
        msg_send![
            ns_string,
            initWithBytes: s.as_ptr()
            length: s.len()
            encoding: NS_UTF8_STRING_ENCODING
        ]
    }

    unsafe fn nsstring_to_string(s: *mut Object) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let cstr_ptr: *const c_char = msg_send![s, UTF8String];
        if cstr_ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(cstr_ptr).to_string_lossy().into_owned())
    }

    unsafe fn dict_object_for_key(dict: *mut Object, key: &str) -> *mut Object {
        let key_ns = nsstring_from_str(key);
        let value: *mut Object = msg_send![dict, objectForKey: key_ns];
        let _: () = msg_send![key_ns, release];
        value
    }

    unsafe fn dict_get_string_with_keys(dict: *mut Object, keys: &[&str]) -> Option<String> {
        for key in keys {
            let value = dict_object_for_key(dict, key);
            if !value.is_null() {
                if let Some(s) = nsstring_to_string(value) {
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
        None
    }

    unsafe fn dict_get_f64_with_keys(dict: *mut Object, keys: &[&str]) -> Option<f64> {
        for key in keys {
            let value = dict_object_for_key(dict, key);
            if !value.is_null() {
                let number: f64 = msg_send![value, doubleValue];
                return Some(number);
            }
        }
        None
    }

    unsafe fn dict_get_object_with_keys(dict: *mut Object, keys: &[&str]) -> *mut Object {
        for key in keys {
            let value = dict_object_for_key(dict, key);
            if !value.is_null() {
                return value;
            }
        }
        std::ptr::null_mut()
    }

    unsafe fn nsdata_to_vec(data: *mut Object) -> Option<Vec<u8>> {
        if data.is_null() {
            return None;
        }
        let len: usize = msg_send![data, length];
        let ptr: *const u8 = msg_send![data, bytes];
        if ptr.is_null() || len == 0 {
            return None;
        }
        Some(std::slice::from_raw_parts(ptr, len).to_vec())
    }

    unsafe fn extract_artwork_data_uri(dict: *mut Object) -> Option<String> {
        let artwork = dict_get_object_with_keys(dict, &["artwork", "MPMediaItemPropertyArtwork"]);
        if artwork.is_null() {
            return None;
        }

        let image: *mut Object = msg_send![
            artwork,
            imageWithSize: CGSize {
                width: 512.0,
                height: 512.0,
            }
        ];
        if image.is_null() {
            return None;
        }

        let tiff_data: *mut Object = msg_send![image, TIFFRepresentation];
        if tiff_data.is_null() {
            return None;
        }

        let bitmap_rep: *mut Object =
            msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff_data];
        if bitmap_rep.is_null() {
            return None;
        }

        let props: *mut Object = msg_send![class!(NSDictionary), dictionary];
        let png_data: *mut Object = msg_send![
            bitmap_rep,
            representationUsingType: NS_BITMAP_IMAGE_FILE_TYPE_PNG
            properties: props
        ];

        let bytes = nsdata_to_vec(png_data)?;
        Some(format!("data:image/png;base64,{}", B64.encode(bytes)))
    }

    fn parse_playback_state(info: *mut Object, title: &str, artist: &str) -> String {
        let playback_rate = unsafe {
            dict_get_f64_with_keys(
                info,
                &["playbackRate", "MPNowPlayingInfoPropertyPlaybackRate"],
            )
            .unwrap_or(0.0)
        };

        if title.is_empty() && artist.is_empty() {
            "stopped".to_owned()
        } else if playback_rate > 0.0 {
            "playing".to_owned()
        } else {
            "paused".to_owned()
        }
    }

    pub fn get_now_playing() -> Option<(String, String, String)> {
        autoreleasepool(|_| unsafe {
            let center: *mut Object = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            if center.is_null() {
                return None;
            }

            let info: *mut Object = msg_send![center, nowPlayingInfo];
            if info.is_null() {
                return None;
            }

            let title = dict_get_string_with_keys(info, &["title", "MPMediaItemPropertyTitle"])
                .unwrap_or_default();
            let artist = dict_get_string_with_keys(info, &["artist", "MPMediaItemPropertyArtist"])
                .unwrap_or_default();
            let state = parse_playback_state(info, &title, &artist);

            Some((state, title, artist))
        })
    }

    pub fn is_playing() -> bool {
        if let Some((state, _, _)) = get_now_playing() {
            state == "playing"
        } else {
            false
        }
    }

    pub fn get_artwork_data_uri() -> Option<String> {
        autoreleasepool(|_| unsafe {
            let center: *mut Object = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            if center.is_null() {
                return None;
            }

            let info: *mut Object = msg_send![center, nowPlayingInfo];
            if info.is_null() {
                return None;
            }

            extract_artwork_data_uri(info)
        })
    }
}

#[cfg(target_os = "macos")]
mod music_scripting {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use objc::{class, msg_send, rc::autoreleasepool, runtime::Object};
    use std::ffi::{c_char, CStr};

    #[link(name = "ScriptingBridge", kind = "framework")]
    extern "C" {}

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    const NS_UTF8_STRING_ENCODING: usize = 4;

    const fn fourcc(bytes: [u8; 4]) -> u32 {
        ((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | (bytes[3] as u32)
    }

    const PLAYER_STOPPED: u32 = fourcc(*b"kPSS");
    const PLAYER_PLAYING: u32 = fourcc(*b"kPSP");
    const PLAYER_PAUSED: u32 = fourcc(*b"kPSp");

    unsafe fn nsstring_from_str(s: &str) -> *mut Object {
        let ns_string: *mut Object = msg_send![class!(NSString), alloc];
        msg_send![
            ns_string,
            initWithBytes: s.as_ptr()
            length: s.len()
            encoding: NS_UTF8_STRING_ENCODING
        ]
    }

    unsafe fn nsstring_to_string(s: *mut Object) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let cstr_ptr: *const c_char = msg_send![s, UTF8String];
        if cstr_ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(cstr_ptr).to_string_lossy().into_owned())
    }

    unsafe fn nsdata_to_vec(data: *mut Object) -> Option<Vec<u8>> {
        if data.is_null() {
            return None;
        }
        let len: usize = msg_send![data, length];
        let ptr: *const u8 = msg_send![data, bytes];
        if ptr.is_null() || len == 0 {
            return None;
        }
        Some(std::slice::from_raw_parts(ptr, len).to_vec())
    }

    pub fn get_now_playing() -> Option<(String, String, String)> {
        autoreleasepool(|_| unsafe {
            let bundle_id = nsstring_from_str("com.apple.Music");
            let app: *mut Object =
                msg_send![class!(SBApplication), applicationWithBundleIdentifier: bundle_id];
            let _: () = msg_send![bundle_id, release];

            if app.is_null() {
                return None;
            }

            let is_running: bool = msg_send![app, isRunning];
            if !is_running {
                return None;
            }

            let player_state_raw: i64 = msg_send![app, playerState];
            let player_state = player_state_raw as u32;

            let track: *mut Object = msg_send![app, currentTrack];
            let (title, artist) = if track.is_null() {
                (String::new(), String::new())
            } else {
                let title_obj: *mut Object = msg_send![track, name];
                let artist_obj: *mut Object = msg_send![track, artist];
                (
                    nsstring_to_string(title_obj).unwrap_or_default(),
                    nsstring_to_string(artist_obj).unwrap_or_default(),
                )
            };

            let state = match player_state {
                PLAYER_PLAYING => "playing",
                PLAYER_PAUSED => "paused",
                PLAYER_STOPPED => "stopped",
                _ => {
                    if !title.is_empty() || !artist.is_empty() {
                        "paused"
                    } else {
                        "stopped"
                    }
                }
            }
            .to_owned();

            Some((state, title, artist))
        })
    }

    unsafe fn get_music_app() -> Option<*mut Object> {
        let bundle_id = nsstring_from_str("com.apple.Music");
        let app: *mut Object =
            msg_send![class!(SBApplication), applicationWithBundleIdentifier: bundle_id];
        let _: () = msg_send![bundle_id, release];

        if app.is_null() {
            return None;
        }

        let is_running: bool = msg_send![app, isRunning];
        if !is_running {
            return None;
        }

        Some(app)
    }

    pub fn play_pause() -> bool {
        autoreleasepool(|_| unsafe {
            let Some(app) = get_music_app() else {
                return false;
            };
            let _: () = msg_send![app, playpause];
            true
        })
    }

    pub fn next_track() -> bool {
        autoreleasepool(|_| unsafe {
            let Some(app) = get_music_app() else {
                return false;
            };
            let _: () = msg_send![app, nextTrack];
            true
        })
    }

    pub fn previous_track() -> bool {
        autoreleasepool(|_| unsafe {
            let Some(app) = get_music_app() else {
                return false;
            };
            let _: () = msg_send![app, backTrack];
            true
        })
    }

    pub fn get_artwork_data_uri() -> Option<String> {
        autoreleasepool(|_| unsafe {
            let bundle_id = nsstring_from_str("com.apple.Music");
            let app: *mut Object =
                msg_send![class!(SBApplication), applicationWithBundleIdentifier: bundle_id];
            let _: () = msg_send![bundle_id, release];

            if app.is_null() {
                return None;
            }

            let is_running: bool = msg_send![app, isRunning];
            if !is_running {
                return None;
            }

            let track: *mut Object = msg_send![app, currentTrack];
            if track.is_null() {
                return None;
            }

            let artworks: *mut Object = msg_send![track, artworks];
            if artworks.is_null() {
                return None;
            }

            let count: usize = msg_send![artworks, count];
            if count == 0 {
                return None;
            }

            let artwork: *mut Object = msg_send![artworks, objectAtIndex: 0usize];
            if artwork.is_null() {
                return None;
            }

            let mut data: *mut Object = msg_send![artwork, data];
            if data.is_null() {
                data = msg_send![artwork, rawData];
            }

            let bytes = nsdata_to_vec(data)?;
            let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "image/jpeg"
            } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                "image/png"
            } else {
                "application/octet-stream"
            };

            Some(format!("data:{};base64,{}", mime, B64.encode(bytes)))
        })
    }
}

#[cfg(target_os = "macos")]
pub fn press_previous() {
    if !music_scripting::previous_track() {
        media_keys::send(media_keys::KEY_PREV);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn press_previous() {}

#[cfg(target_os = "macos")]
pub fn press_next() {
    if !music_scripting::next_track() {
        media_keys::send(media_keys::KEY_NEXT);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn press_next() {}

#[cfg(target_os = "macos")]
pub fn press_play_pause() {
    if !music_scripting::play_pause() {
        media_keys::send(media_keys::KEY_PLAY);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn press_play_pause() {}

#[cfg(target_os = "macos")]
pub fn is_muted() -> bool {
    coreaudio_system::output_muted().unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn is_muted() -> bool {
    false
}

#[cfg(target_os = "macos")]
pub fn is_media_playing() -> bool {
    get_now_playing().state == "playing"
}

#[cfg(not(target_os = "macos"))]
pub fn is_media_playing() -> bool {
    false
}

#[cfg(target_os = "macos")]
pub fn adjust_volume(delta: i32) {
    if let Some(current) = coreaudio_system::output_volume() {
        let next = (current + (delta as f32 / 100.0)).clamp(0.0, 1.0);
        let _ = coreaudio_system::set_output_volume(next);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn adjust_volume(_delta: i32) {}

#[cfg(target_os = "macos")]
pub fn current_volume_percent() -> Option<f32> {
    coreaudio_system::output_volume().map(|v| (v * 100.0).clamp(0.0, 100.0))
}

#[cfg(not(target_os = "macos"))]
pub fn current_volume_percent() -> Option<f32> {
    None
}

#[cfg(target_os = "macos")]
pub fn toggle_mute() {
    if let Some(muted) = coreaudio_system::output_muted() {
        let _ = coreaudio_system::set_output_muted(!muted);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn toggle_mute() {}

#[cfg(target_os = "macos")]
pub fn get_artwork_b64() -> Option<String> {
    applescript_fallback::get_artwork_data_uri()
}

#[cfg(not(target_os = "macos"))]
pub fn get_artwork_b64() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
pub fn get_now_playing() -> NowPlaying {
    if let Some((state, title, artist)) = music_scripting::get_now_playing() {
        return NowPlaying {
            state,
            title,
            artist,
        };
    }

    NowPlaying {
        state: "stopped".into(),
        title: String::new(),
        artist: String::new(),
    }
}

#[cfg(target_os = "macos")]
pub fn get_now_playing_progress() -> Option<PlaybackProgress> {
    let (elapsed_secs, duration_secs) = applescript_fallback::get_now_playing_progress()?;
    let remaining_secs = (duration_secs - elapsed_secs).max(0.0);
    let progress_percent = (elapsed_secs / duration_secs * 100.0).clamp(0.0, 100.0);
    Some(PlaybackProgress {
        elapsed_secs,
        duration_secs,
        remaining_secs,
        progress_percent,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn get_now_playing_progress() -> Option<PlaybackProgress> {
    None
}

#[cfg(target_os = "macos")]
pub fn get_battery_status() -> Option<BatteryStatus> {
    let out = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let percent = text
        .split('%')
        .next()?
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse::<f32>()
        .ok()?;

    let lower = text.to_lowercase();
    let is_charging = lower.contains("charging") || lower.contains("ac power");
    Some(BatteryStatus {
        percent: percent.clamp(0.0, 100.0),
        is_charging,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn get_battery_status() -> Option<BatteryStatus> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn get_now_playing() -> NowPlaying {
    NowPlaying {
        state: "stopped".into(),
        title: String::new(),
        artist: String::new(),
    }
}