use crate::error::Error;
use serde::Deserialize;
use std::path::PathBuf;

/// Config of mpv-handler
///
/// - `mpv`: mpv binary path
/// - `ytdl`: yt-dlp binary path
/// - `proxy: HTTP(S) proxy server address
#[derive(Debug, Deserialize)]
pub struct Config {
    pub mpv: Option<String>,
    pub ytdl: Option<String>,
    pub proxy: Option<String>,
    pub socket: Option<String>,
}

impl Config {
    /// Load config file and retruns `Config`
    ///
    /// If config file doesn't exists, returns default value
    pub fn load() -> Result<Config, Error> {
        if let Some(mut path) = get_config_dir() {
            path.push("config.toml");

            if path.exists() {
                let data: String = std::fs::read_to_string(&path)?;
                let mut config: Config = toml::from_str(&data)?;

                if let Some(mpv) = config.mpv {
                    config.mpv = Some(realpath(mpv)?);
                }
                if let Some(ytdl) = config.ytdl {
                    config.ytdl = Some(realpath(ytdl)?);
                }

                if config.socket.is_none() {
                    config.socket = Some(default_socket());
                }

                return Ok(config);
            }
        }

        Ok(default_config())
    }
}

/// Returns config directory path of mpv-handler
pub fn get_config_dir() -> Option<PathBuf> {
    // Linux config directory location: $XDG_CONFIG_HOME/mpv-handler/
    #[cfg(unix)]
    {
        if let Some(mut v) = dirs::config_dir() {
            v.push("mpv-handler");
            return Some(v);
        }
    }

    // Windows config directory location: %WORKING_DIR%\
    #[cfg(windows)]
    {
        if let Ok(mut v) = std::env::current_exe() {
            v.pop();
            return Some(v);
        }
    }

    eprintln!("Failed to get config directory");
    None
}

/// The default value of `Config.mpv`
pub fn default_mpv() -> Result<String, Error> {
    #[cfg(unix)]
    return realpath("mpv");
    #[cfg(windows)]
    return realpath("mpv.com");
}

/// The default value of `Config.socket`
pub fn default_socket() -> String {
    #[cfg(unix)]
    return "/tmp/mpvsocket".to_string();
    #[cfg(windows)]
    return r"\\.\pipe\mpvsocket".to_string();
}

/// The defalut value of `Config`
fn default_config() -> Config {
    Config {
        mpv: None,
        ytdl: None,
        proxy: None,
        socket: Some(default_socket()),
    }
}

/// Find and read `ytdl-format` from `mpv.conf`
pub fn get_ytdl_format_from_mpv_conf() -> Option<String> {
    // Get mpv config directory
    let mut path = dirs::config_dir()?;
    path.push("mpv/mpv.conf");

    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;

    // Find `ytdl-format` option
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("ytdl-format=") {
            let value = line.split_at("ytdl-format=".len()).1.trim();
            return Some(value.to_string());
        }
    }

    None
}

fn realpath<T: AsRef<std::ffi::OsStr>>(path: T) -> Result<String, Error> {
    let path = std::path::PathBuf::from(&path);

    if path.is_relative() {
        #[cfg(windows)]
        {
            if let Some(mut p) = crate::config::get_config_dir() {
                p.push(&path);
                if let Ok(rp) = p.canonicalize() {
                    return Ok(rp.display().to_string());
                };
            }
        }

        if let Some(paths) = std::env::var_os("PATH") {
            for mut p in std::env::split_paths(&paths) {
                p.push(&path);
                if let Ok(rp) = p.canonicalize() {
                    return Ok(rp.display().to_string());
                };
            }
        }
    }

    Ok(path.display().to_string())
}

#[test]
fn test_config_parse() {
    // Custom values
    let config: Config = toml::from_str(
        r#"
            mpv = "/usr/bin/mpv"
            ytdl = "/usr/bin/yt-dlp"
            proxy = "http://example.com:8080"
            socket = "/tmp/mpv"
        "#,
    )
    .unwrap();

    assert_eq!(config.mpv, Some("/usr/bin/mpv".to_string()));
    assert_eq!(config.ytdl, Some("/usr/bin/yt-dlp".to_string()));
    assert_eq!(config.proxy, Some("http://example.com:8080".to_string()));
    assert_eq!(config.socket, Some("/tmp/mpv".to_string()));

    // Unexpected values
    let config: Config = toml::from_str(
        r#"
            key1 = "value1"
            key2 = "value2"
            key3 = "value3"
        "#,
    )
    .unwrap();

    #[cfg(unix)]
    assert_eq!(config.mpv, None);
    assert_eq!(config.ytdl, None);
    assert_eq!(config.proxy, None);
    assert_eq!(config.socket, None);
}
