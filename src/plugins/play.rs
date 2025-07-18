use crate::config::Config;
use crate::error::Error;
use crate::protocol::Protocol;
use serde_json::json;
use std::io::prelude::*;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::fs;
use std::path::PathBuf;

const PREFIX_COOKIES: &str = "--ytdl-raw-options-append=cookies=";
const PREFIX_PROFILE: &str = "--profile=";
const PREFIX_FORMATS: &str = "--ytdl-raw-options-append=format-sort=";
const PREFIX_V_TITLE: &str = "--title=";
const PREFIX_SUBFILE: &str = "--sub-file=";
const PREFIX_STARTAT: &str = "--start=";
const PREFIX_YT_PATH: &str = "--script-opts=ytdl_hook-ytdl_path=";

fn get_mpv_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut path| {
        path.push("mpv");
        path.push("mpv.conf");
        path
    })
}

fn get_ytdl_format_from_mpv_conf() -> Option<String> {
    let config_path = get_mpv_config_path()?;
    eprintln!("Checking for mpv.conf at: {}", config_path.display());
    let content = fs::read_to_string(config_path).ok()?;
    for line in content.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with('#') || trimmed_line.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed_line.split_once('=') {
            if key.trim() == "ytdl-format" {
                let format = value.trim().to_string();
                eprintln!("Found ytdl-format in mpv.conf: {}", &format);
                return Some(format);
            }
        }
    }
    eprintln!("ytdl-format not found in mpv.conf, using default.");
    None
}


/// Execute player with given options
pub fn exec(proto: &Protocol, config: &Config) -> Result<(), Error> {
    let ytdl_path = config.ytdl.as_deref().unwrap_or("yt-dlp");
    eprintln!("Using yt-dlp path: {}", ytdl_path);

    let mut is_playlist = false;
    let mut playlist_entries: Vec<(String, String)> = Vec::new(); // (title, url)

    // First, check if it's a playlist and get all entries
    let playlist_check_output = Command::new(ytdl_path)
        .arg("--flat-playlist")
        .arg("--dump-json")
        .arg(&proto.url)
        .output();

    if let Ok(output) = playlist_check_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
                    if let (Some(title), Some(url)) = (
                        json_value["title"].as_str(),
                        json_value["url"].as_str(),
                    ) {
                        if title != "[Deleted video]" && title != "[Private video]" {
                            playlist_entries.push((title.to_string(), url.to_string()));
                        } else {
                            eprintln!("Skipping unavailable video: {}", title);
                        }
                    }
                }
            }
            if playlist_entries.len() > 1 {
                is_playlist = true;
                eprintln!("Detected playlist with {} entries.", playlist_entries.len());
            }
        }
    }

    // --- Determine if we use existing socket or launch new instance ---
    let mut use_existing_socket = false;
    if proto.enqueue == Some(true) {
        if let Some(socket_path) = &config.socket {
            if UnixStream::connect(socket_path).is_ok() {
                use_existing_socket = true;
                eprintln!("Connected to existing mpv socket: {}", socket_path);
            } else {
                eprintln!("No existing mpv socket found or connection failed. Launching new instance.");
            }
        }
    }

    let ytdl_format = get_ytdl_format_from_mpv_conf()
        .unwrap_or_else(|| "bestvideo[height<=?1920][fps<=?30][vcodec^=avc]+bestaudio/best".to_string());

    let mut initial_url_to_play: String;
    let mut initial_title: String;
    let mut initial_audio_url: Option<String> = None;

    if !use_existing_socket && is_playlist {
        // For a new mpv instance with a playlist, pass the original video URL to mpv
        // and let its ytdl-hook handle the extraction, as requested.
        eprintln!("Launching new instance for playlist, passing first video URL directly to mpv.");
        initial_url_to_play = playlist_entries[0].1.clone();
        initial_title = playlist_entries[0].0.clone();
    } else {
        // For all other cases (single video, or enqueuing to existing instance),
        // we fetch the direct URL ourselves to have more control.
        let url_for_ytdl_fetch = if is_playlist {
            &playlist_entries[0].1
        } else {
            &proto.url
        };

        // Set initial values as a fallback
        initial_url_to_play = url_for_ytdl_fetch.to_string();
        initial_title = if is_playlist { playlist_entries[0].0.clone() } else { proto.url.clone() };

        eprintln!("Fetching direct URL for: {}", url_for_ytdl_fetch);
        let ytdl_output = Command::new(ytdl_path)
            .arg("-f").arg(&ytdl_format)
            .arg("--get-url")
            .arg("--check-formats")
            .arg("--get-title")
            .arg(url_for_ytdl_fetch)
            .output();

        match ytdl_output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.trim().lines().collect();
                if lines.len() >= 2 {
                    initial_title = lines[0].to_string();
                    initial_url_to_play = lines[1].to_string();
                    initial_audio_url = if lines.len() >= 3 { Some(lines[2].to_string()) } else { None };
                    eprintln!("Extracted Title: {}", initial_title);
                    eprintln!("Extracted Video URL: {}", initial_url_to_play);
                    if let Some(ref audio) = initial_audio_url {
                        eprintln!("Extracted Audio URL: {}", audio);
                    }
                } else {
                    eprintln!("yt-dlp returned insufficient output. Using pre-fetched data as fallback.");
                }
            }
            _ => {
                eprintln!("Failed to execute yt-dlp or it returned an error. Using pre-fetched data as fallback.");
            }
        };
    }

    if use_existing_socket {
        // --- Enqueue to existing socket ---
        if let Some(socket_path) = &config.socket {
            if let Ok(mut stream) = UnixStream::connect(socket_path) {
                eprintln!("Enqueuing to existing mpv instance.");
                // When enqueuing, we always fetch the direct URL for the item.
                let mut command_parts: Vec<serde_json::Value> = vec![
                    json!("loadfile"),
                    json!(initial_url_to_play),
                    json!("append"), // Do not play immediately
                ];

                let mut options_obj = serde_json::Map::new();
                options_obj.insert("title".to_string(), json!(initial_title));

                if let Some(audio) = initial_audio_url {
                    options_obj.insert("audio-file".to_string(), json!(audio));
                }

                if !options_obj.is_empty() {
                    command_parts.push(json!(options_obj));
                }

                let command = json!({ "command": command_parts });
                let command_str = command.to_string() + "\n";
                eprintln!("Sending command to mpv: {}", command_str.trim());
                stream.write_all(command_str.as_bytes())?;
                println!("Enqueued: {}", initial_title);

                // If it's a playlist, we also pre-fetch and enqueue the rest of the items
                if is_playlist {
                    for i in 1..playlist_entries.len() {
                        let (title, url) = &playlist_entries[i];
                        eprintln!("Enqueuing subsequent playlist item: {} - {}", title, url);

                        let ytdl_output = Command::new(ytdl_path)
                            .arg("-f").arg(&ytdl_format)
                            .arg("--get-url")
                            .arg("--check-formats")
                            .arg("--get-title")
                            .arg(url)
                            .output();

                        let (video_title, video_url, audio_url) = match ytdl_output {
                            Ok(output) if output.status.success() => {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                let lines: Vec<&str> = stdout.trim().lines().collect();
                                if lines.len() >= 2 {
                                    (lines[0].to_string(), lines[1].to_string(), lines.get(2).map(|s| s.to_string()))
                                } else {
                                    (title.clone(), url.clone(), None)
                                }
                            }
                            _ => (title.clone(), url.clone(), None),
                        };

                        let mut options_obj = serde_json::Map::new();
                        options_obj.insert("title".to_string(), json!(video_title));
                        if let Some(audio) = audio_url {
                            options_obj.insert("audio-file".to_string(), json!(audio));
                        }

                        let command = json!({ "command": ["loadfile", video_url, "append", options_obj] });
                        let command_str = command.to_string() + "\n";
                        eprintln!("Sending command to mpv: {}", command_str.trim());
                        stream.write_all(command_str.as_bytes())?;
                        println!("Enqueued: {} - {}", video_title, url);
                    }
                }
                return Ok(()); // Successfully handled via existing socket
            }
        }
    }

    // --- Launch new mpv instance ---
    let mut options: Vec<String> = Vec::new();

    if let Some(v) = proto.cookies { if let Some(v) = cookies(v) { options.push(v); } }
    if let Some(v) = proto.profile { options.push(profile(v)); }
    if proto.quality.is_some() || proto.v_codec.is_some() { if let Some(v) = formats(proto.quality, proto.v_codec) { options.push(v); } }
    if let Some(v) = &proto.v_title { options.push(v_title(v)); }
    if let Some(v) = &proto.subfile { options.push(subfile(v)); }
    if let Some(v) = &proto.startat { options.push(startat(v)); }
    if let Some(v) = &config.ytdl { options.push(yt_path(v)); }

    if !use_existing_socket && is_playlist {
        options.push("--idle=yes".to_string());
    }

    if &proto.scheme == &crate::protocol::Schemes::MpvDebug || cfg!(debug_assertions) {
        // ... (debug output remains the same)
    }

    if proto.enqueue == Some(true) { if let Some(socket_path) = &config.socket { options.push(format!("--input-ipc-server={}", socket_path)); } }

    // Only add audio-file if we have an extracted URL for it.
    // This won't be the case for the first video in a new playlist instance.
    if let Some(audio_url) = &initial_audio_url {
        options.push(format!("--audio-file={}", audio_url));
    }

    let mut command = std::process::Command::new(config.mpv.as_deref().unwrap_or("mpv"));
    command.args(&options);

    // Only pass a URL on the command line if we are NOT launching a new playlist instance.
    if !(!use_existing_socket && is_playlist) {
        println!("Playing: {}", initial_url_to_play);
        command.arg("--").arg(&initial_url_to_play);
    }

    if let Some(proxy) = &config.proxy {
        command.env("http_proxy", proxy).env("HTTP_PROXY", proxy).env("https_proxy", proxy).env("HTTPS_PROXY", proxy);
    }

    #[cfg(unix)]
    command.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD");

    match command.spawn() {
        Ok(mut child) => {
            let mut _stream_guard: Option<UnixStream> = None;

            if !use_existing_socket && is_playlist {
                if let Some(socket_path) = &config.socket {
                    let mut stream = None;
                    for i in 0..15 { // Retry connecting for ~3 seconds
                        if let Ok(s) = UnixStream::connect(socket_path) {
                            eprintln!("Connected to mpv socket after {}ms.", i * 200);
                            stream = Some(s);
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }

                    if let Some(mut s) = stream {
                        // 1. Load the first video
                        println!("Playing: {}", initial_url_to_play);
                        let first_cmd = json!({ "command": ["loadfile", initial_url_to_play, "replace", { "title": initial_title }] });
                        if let Err(e) = s.write_all((first_cmd.to_string() + "\n").as_bytes()) {
                            eprintln!("Failed to send first command to mpv: {}", e);
                        } else {
                            // 2. Enqueue the rest of the items
                            for i in 1..playlist_entries.len() {
                                let (title, url) = &playlist_entries[i];
                                let (video_title, video_url, audio_url) = {
                                    let ytdl_output = Command::new(ytdl_path).arg("-f").arg(&ytdl_format).arg("--get-url").arg("--check-formats").arg("--get-title").arg(url).output();
                                    match ytdl_output {
                                        Ok(output) if output.status.success() => {
                                            let stdout_str = String::from_utf8_lossy(&output.stdout);
                                            let lines: Vec<&str> = stdout_str.trim().lines().collect();
                                            if lines.len() >= 2 { (lines[0].to_string(), lines[1].to_string(), lines.get(2).map(|s| s.to_string())) } else { (title.clone(), url.clone(), None) }
                                        },
                                        _ => (title.clone(), url.clone(), None),
                                    }
                                };
                                let mut opts = serde_json::Map::new();
                                opts.insert("title".to_string(), json!(video_title));
                                if let Some(audio) = audio_url { opts.insert("audio-file".to_string(), json!(audio)); }
                                let cmd = json!({ "command": ["loadfile", video_url, "append", opts] });
                                if let Err(e) = s.write_all((cmd.to_string() + "\n").as_bytes()) {
                                    eprintln!("Failed to enqueue '{}': {}", title, e);
                                    break;
                                }
                                println!("Enqueued: {}", title);
                            }
                        }
                        // Keep the stream alive until mpv exits
                        _stream_guard = Some(s);
                    } else {
                        return Err(Error::SocketConnectionFailed);
                    }
                }
            }
            // Wait for the mpv process to exit. _stream_guard is still in scope.
            let status = child.wait().map_err(Error::PlayerRunFailed)?;
            if !status.success() {
                return Err(Error::PlayerExited(status.code().unwrap_or(1) as u8));
            }
            Ok(())
        },
        Err(e) => Err(Error::PlayerRunFailed(e)),
    }
}



fn cookies(cookies: &str) -> Option<String> {
    match crate::config::get_config_dir() {
        Some(mut p) => {
            p.push("cookies");
            p.push(cookies);

            if p.exists() {
                let cookies = p.display();
                return Some(format!("{PREFIX_COOKIES}{cookies}"));
            } else {
                eprintln!("Cookies file not found: {}", p.display());
                return None;
            }
        }
        None => None,
    }
}

fn profile(profile: &str) -> String {
    format!("{PREFIX_PROFILE}{profile}")
}

fn formats(quality: Option<&str>, v_codec: Option<&str>) -> Option<String> {
    let mut f: Vec<String> = Vec::new();
    if let Some(v) = quality {
        let i: String = v.matches(char::is_numeric).collect();
        f.push(format!("res:{}", i));
    }
    if let Some(v) = v_codec {
        f.push(format!("+vcodec:{}", v))
    }
    if f.is_empty() {
        None
    } else {
        Some(format!("{PREFIX_FORMATS}{}", f.join(",")))
    }
}

fn v_title(v_title: &str) -> String {
    format!("{PREFIX_V_TITLE}{v_title}")
}

fn subfile(subfile: &str) -> String {
    format!("{PREFIX_SUBFILE}{subfile}")
}

fn startat(startat: &str) -> String {
    format!("{PREFIX_STARTAT}{startat}")
}

fn yt_path(yt_path: &str) -> String {
    format!("{PREFIX_YT_PATH}{yt_path}")
}

#[test]
fn test_profile_option() {
    let p = profile("low-latency");
    assert_eq!(p, "--profile=low-latency");
}

#[test]
fn test_formats_option() {
    let q = formats(Some("720p"), None);
    assert_eq!(q.unwrap(), "--ytdl-raw-options-append=format-sort=res:720");

    let v = formats(None, Some("vp9"));
    assert_eq!(v.unwrap(), "--ytdl-raw-options-append=format-sort=+vcodec:vp9");

    let qv = formats(Some("720p"), Some("vp9"));
    assert_eq!(qv.unwrap(), "--ytdl-raw-options-append=format-sort=res:720,+vcodec:vp9");
}

#[test]
fn test_v_title_option() {
    let t = v_title("Hello World!");
    assert_eq!(t, "--title=Hello World!");
}

#[test]
fn test_subfile_option() {
    let s = subfile("http://example.com/en.ass");
    assert_eq!(s, "--sub-file=http://example.com/en.ass");
}

#[test]
fn test_startat_option() {
    let s = startat("233");
    assert_eq!(s, "--start=233");
}

#[test]
fn test_yt_path_option() {
    let y = yt_path("/usr/bin/yt-dlp");
    assert_eq!(y, "--script-opts=ytdl_hook-ytdl_path=/usr/bin/yt-dlp");
}
