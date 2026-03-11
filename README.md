# macOS Utilities — OpenDeck Plugin

Control macOS media playback and system volume from your OpenDeck or Stream Deck device.

**Author:** Niklas Arnitz

## Actions

| Action                        | State                                               |
| ----------------------------- | --------------------------------------------------- |
| Previous Track                | —                                                   |
| Next Track                    | —                                                   |
| Play / Pause                  | Live: reflects current playback state               |
| Volume Up                     | —                                                   |
| Volume Down                   | —                                                   |
| Mute Toggle                   | Live: reflects current mute state                   |
| Now Playing                   | Live: shows track name + artist (updates every 2 s) |
| Next Calendar Event (Infobar) | Live: shows your next upcoming macOS Calendar event |

## Requirements

**End users (install from zip):**

- macOS 12 or later
- No additional dependencies

**Developers (build from source):**

- Rust — `brew install rust`

## Installation

Download `macos-utilities.zip` from the [Releases](../../releases) page, then install via **OpenDeck → Settings → Install ZIP**.

## Building from source

```sh
git clone https://github.com/niklasarnitz/macos-utilities
cd macos-utilities
./build.sh
```

The built zip is written to `dist/macos-utilities.zip`.

## How it works

| Capability                 | Implementation                                                                                         |
| -------------------------- | ------------------------------------------------------------------------------------------------------ |
| Play / Pause / Next / Prev | Rust sends `NX_KEYTYPE_*` system media key events via NSEvent + CGEventPost — works with any media app |
| Volume Up / Down           | AppleScript: reads and writes `output volume` of system volume settings                                |
| Mute toggle                | AppleScript: flips `output muted` on system volume settings                                            |
| Play state detection       | AppleScript checks `player state` of Music, Spotify, Podcasts, TV                                      |
| Mute state detection       | AppleScript reads `output muted`                                                                       |
| Now Playing                | AppleScript queries Music or Spotify for current track metadata                                        |

## Permissions

macOS may prompt for **Accessibility** permission the first time media keys are sent. If play/pause/next/prev do not work, go to **System Settings → Privacy & Security → Accessibility** and allow the OpenDeck application to control your computer.
