use crate::config::Config;
use crate::error::Error;
use crate::protocol::Protocol;
use serde_json::json;
use std::borrow::Cow;
use std::fs;
use std::io::prelude::*;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;

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

    // --- Playlist Detection ---
    let mut is_playlist = false;
    let mut playlist_entries: Vec<(String, String)> = Vec::new(); // (title, url)

    let is_explicit_playlist = proto.url.contains("&list=");

    if is_explicit_playlist {
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
                    let total_entries = playlist_entries.len();
                    let dialog_text = format!(
                        "Playlist detected with {} entries.\nHow many items do you want to fetch? (0 for all)",
                        total_entries
                    );
                    let confirmation_output = Command::new("zenity")
                        .arg("--entry")
                        .arg("--text")
                        .arg(&dialog_text)
                        .arg("--entry-text")
                        .arg("0") // Default value is 0
                        .arg("--cancel-label=Play only the first video")
                        .arg("--timeout=10")
                        .output();

                    match confirmation_output {
                        Ok(output) => {
                            match output.status.code() {
                                Some(0) => { // OK clicked
                                    let num_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    match num_str.parse::<usize>() {
                                        Ok(0) => {
                                            is_playlist = true;
                                            eprintln!("User chose to fetch all {} playlist items.", total_entries);
                                        }
                                        Ok(num) => {
                                            is_playlist = true;
                                            playlist_entries.truncate(num);
                                            eprintln!("User chose to fetch the first {} playlist items.", playlist_entries.len());
                                        }
                                        Err(_) => {
                                            is_playlist = false;
                                            eprintln!("Invalid input. Treating as a single video.");
                                        }
                                    }
                                }
                                Some(5) => { // Timeout
                                    is_playlist = true;
                                    eprintln!("Dialog timed out. Fetching all {} playlist items by default.", total_entries);
                                }
                                _ => { // Cancelled or other error
                                    is_playlist = false;
                                    eprintln!("User cancelled or dialog failed. Treating as a single video.");
                                }
                            }
                        }
                        Err(e) => { // Failed to execute zenity
                            is_playlist = false;
                            eprintln!("Zenity command failed: {}. Treating as a single video.", e);
                        }
                    }
                }
            }
        }
    }

    // --- Socket Check ---
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

    // --- Main Logic ---

    if use_existing_socket {
        // --- Enqueue to Existing Instance ---
        let items_to_enqueue: Cow<[(String, String)]> = if is_playlist {
            Cow::Borrowed(&playlist_entries)
        } else {
            Cow::Owned(vec![(proto.v_title.clone().unwrap_or(proto.url.clone()), proto.url.clone())]) // Use proto.v_title or URL as title for single video
        };

        if let Some(socket_path) = &config.socket {
            if let Ok(mut stream) = UnixStream::connect(socket_path) {
                eprintln!("Enqueuing to existing mpv instance.");
                for (index, (initial_title, url)) in items_to_enqueue.iter().enumerate() {
                    eprintln!("Enqueuing item [{}]: {} - {}", index + 1, initial_title, url);

                    let video_url: String;
                    let audio_url: Option<String>;
                    let display_title: String;

                    if is_playlist {
                        // For playlist items, fetch direct URLs for performance, but use the pre-fetched title
                        let (fetched_title, fetched_video_url, fetched_audio_url) = fetch_direct_urls(ytdl_path, &ytdl_format, url, initial_title);
                        video_url = fetched_video_url;
                        audio_url = fetched_audio_url;
                        display_title = initial_title.clone(); // Use the title from playlist_entries
                    } else {
                        // For single videos, prefetch direct URLs
                        let (fetched_title, fetched_video_url, fetched_audio_url) = fetch_direct_urls(ytdl_path, &ytdl_format, url, initial_title);
                        video_url = fetched_video_url;
                        audio_url = fetched_audio_url;
                        display_title = fetched_title;
                    };

                    let mut options_obj = serde_json::Map::new();
                    options_obj.insert("title".to_string(), json!(display_title.clone())); // Use display_title for OSC
                    if let Some(audio) = audio_url {
                        options_obj.insert("audio-file".to_string(), json!(audio));
                    }

                    let load_command = json!({ "command": ["loadfile", video_url, "append", options_obj] });
                    let set_playlist_title_command = json!({ "command": ["set_property", "playlist/-1/title", display_title] }); // Use display_title for playlist

                    stream.write_all((load_command.to_string() + "
").as_bytes())?;
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    stream.write_all((set_playlist_title_command.to_string() + "
").as_bytes())?;

                    println!("Enqueued: {}", display_title); // Print the display title
                }
                return Ok(());
            }
        }
        // Fallthrough to launch new instance if socket connection fails unexpectedly
    }

    // --- Launch New Instance ---
    let mut options: Vec<String> = build_mpv_options(proto, config);

    if is_playlist {
        // --- New Instance for Playlist ---
        options.push("--idle=yes".to_string());
        if proto.enqueue == Some(true) {
            if let Some(socket_path) = &config.socket {
                options.push(format!("--input-ipc-server={}", socket_path));
            }
        }

        let mut command = std::process::Command::new(config.mpv.as_deref().unwrap_or("mpv"));
        command.args(&options);
        if let Some(proxy) = &config.proxy {
            command.env("http_proxy", proxy).env("HTTP_PROXY", proxy).env("https_proxy", proxy).env("HTTPS_PROXY", proxy);
        }
        #[cfg(unix)]
        command.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD");

        match command.spawn() {
            Ok(mut child) => {
                handle_playlist_in_new_instance(
                    &mut child,
                    config,
                    &playlist_entries,
                    ytdl_path,
                    &ytdl_format,
                )?;
                let status = child.wait().map_err(Error::PlayerRunFailed)?;
                if !status.success() {
                    return Err(Error::PlayerExited(status.code().unwrap_or(1) as u8));
                }
                Ok(())
            },
            Err(e) => Err(Error::PlayerRunFailed(e)),
        }
    } else {
        // --- New Instance for Single Video ---
        if proto.enqueue == Some(true) {
            if let Some(socket_path) = &config.socket {
                options.push(format!("--input-ipc-server={}", socket_path));
            }
        }

        let mut command = std::process::Command::new(config.mpv.as_deref().unwrap_or("mpv"));
        command.args(&options);
        // Pass original URL directly to mpv
        command.arg("--").arg(&proto.url);

        if let Some(proxy) = &config.proxy {
            command.env("http_proxy", proxy).env("HTTP_PROXY", proxy).env("https_proxy", proxy).env("HTTPS_PROXY", proxy);
        }
        #[cfg(unix)]
        command.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD");

        let status = command.status().map_err(Error::PlayerRunFailed)?;
        if !status.success() {
            return Err(Error::PlayerExited(status.code().unwrap_or(1) as u8));
        }
        Ok(())
    }
}

/// Helper to fetch direct URLs and title using yt-dlp
fn fetch_direct_urls(ytdl_path: &str, ytdl_format: &str, url: &str, default_title: &str) -> (String, String, Option<String>) {
    eprintln!("Fetching direct URL for: {}", url);
    let ytdl_output = Command::new(ytdl_path)
        .arg("-f").arg(ytdl_format)
        .arg("--get-url")
        .arg("--check-formats")
        .arg("--get-title")
        .arg(url)
        .output();

    match ytdl_output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.trim().lines().collect();
            if lines.len() >= 2 {
                let title = lines[0].to_string();
                let video_url = lines[1].to_string();
                let audio_url = if lines.len() >= 3 { Some(lines[2].to_string()) } else { None };
                eprintln!("Extracted Title: {}", title);
                eprintln!("Extracted Video URL: {}", video_url);
                if let Some(ref audio) = audio_url {
                    eprintln!("Extracted Audio URL: {}", audio);
                }
                (title, video_url, audio_url)
            } else {
                eprintln!("yt-dlp returned insufficient output. Using original URL as fallback.");
                (default_title.to_string(), url.to_string(), None)
            }
        }
        _ => {
            eprintln!("Failed to execute yt-dlp or it returned an error. Using original URL as fallback.");
            (default_title.to_string(), url.to_string(), None)
        }
    }
}

/// Helper to build the initial mpv command line options
fn build_mpv_options(proto: &Protocol, config: &Config) -> Vec<String> {
    let mut options: Vec<String> = Vec::new();
    if let Some(v) = proto.cookies { if let Some(v) = cookies(v) { options.push(v); } }
    if let Some(v) = proto.profile { options.push(profile(v)); }
    if proto.quality.is_some() || proto.v_codec.is_some() { if let Some(v) = formats(proto.quality, proto.v_codec) { options.push(v); } }
    if let Some(v) = &proto.v_title { options.push(v_title(v)); }
    if let Some(v) = &proto.subfile { options.push(subfile(v)); }
    if let Some(v) = &proto.startat { options.push(startat(v)); }
    if let Some(v) = &config.ytdl { options.push(yt_path(v)); }
    if &proto.scheme == &crate::protocol::Schemes::MpvDebug || cfg!(debug_assertions) {
        // ... (debug output remains the same)
    }
    options
}

/// Helper to manage a new mpv instance for a playlist
fn handle_playlist_in_new_instance(
    child: &mut std::process::Child,
    config: &Config,
    playlist_entries: &[(String, String)],
    ytdl_path: &str,
    ytdl_format: &str,
) -> Result<(), Error> {
    if let Some(socket_path) = &config.socket {
        // Wait for the socket to be created
        let mut stream = None;
        for i in 0..15 { // Retry connecting for ~3 seconds
            if let Ok(s) = UnixStream::connect(socket_path) {
                eprintln!("Connected to new mpv socket after {}ms.", i * 200);
                stream = Some(s);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        if let Some(mut s) = stream {
            // 1. Load the first video (don't pre-extract, let mpv do it)
            let (first_title, first_url) = &playlist_entries[0];
            println!("Playing: {}", first_url);
            let first_cmd = json!({ "command": ["loadfile", first_url, "replace", { "title": first_title }] });
            s.write_all((first_cmd.to_string() + "
").as_bytes())?;

            // 2. Enqueue the rest of the items (pre-extracting for performance)
            for (title, url) in playlist_entries.iter().skip(1) {
                let (video_title, video_url, audio_url) = fetch_direct_urls(ytdl_path, ytdl_format, url, title);
                let mut opts = serde_json::Map::new();
                opts.insert("title".to_string(), json!(video_title.clone()));
                if let Some(audio) = audio_url {
                    opts.insert("audio-file".to_string(), json!(audio));
                }

                let load_cmd = json!({ "command": ["loadfile", video_url, "append", opts] });
                let set_playlist_title_cmd = json!({ "command": ["set_property", "playlist/-1/title", video_title] });

                if let Err(e) = s.write_all((load_cmd.to_string() + "
").as_bytes()) {
                    eprintln!("Failed to enqueue '{}': {}", title, e);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Err(e) = s.write_all((set_playlist_title_cmd.to_string() + "
").as_bytes()) {
                    eprintln!("Failed to set playlist title for '{}': {}", title, e);
                    break;
                }
                println!("Enqueued: {}", title);
            }
            // Keep the stream alive until mpv exits by not dropping it.
            // We can't easily wait for the child and hold the stream, so we detach.
            // This is a simplification; a more robust solution might use threads.
            std::mem::forget(s);
        } else {
            // If we can't connect, kill the idle mpv instance
            child.kill().ok();
            return Err(Error::SocketConnectionFailed);
        }
    }
    Ok(())
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
