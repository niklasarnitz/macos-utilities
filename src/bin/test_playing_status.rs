#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc2 as objc;

#[path = "../system_media.rs"]
mod system_media;

use std::env;

fn usage() {
    eprintln!("Usage: cargo run --bin test_playing_status -- [--expect playing|not-playing]");
}

fn main() {
    if !cfg!(target_os = "macos") {
        eprintln!("This test binary only supports macOS.");
        std::process::exit(2);
    }

    let args: Vec<String> = env::args().collect();

    let expected = if args.len() == 1 {
        None
    } else if args.len() == 3 && args[1] == "--expect" {
        match args[2].as_str() {
            "playing" | "not-playing" => Some(args[2].as_str()),
            _ => {
                usage();
                std::process::exit(2);
            }
        }
    } else {
        usage();
        std::process::exit(2);
    };

    let info = system_media::get_now_playing();
    let playing = system_media::is_media_playing();

    println!("state={}", info.state);
    println!("title={}", info.title);
    println!("artist={}", info.artist);
    println!("is_playing={}", playing);

    if let Some(expected) = expected {
        let expected_bool = expected == "playing";
        if playing != expected_bool {
            eprintln!(
                "FAIL: expected is_playing={} but got {}",
                expected_bool, playing
            );
            std::process::exit(1);
        }
        println!("PASS");
    }
}
