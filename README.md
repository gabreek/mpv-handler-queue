# mpv handler (queue mod)

A protocol handler for **mpv**, written in Rust, with an added queueing modification.

This modified version automatically detects if an mpv instance is already running with a socket open. If so, it enqueues the video to the existing instance. Otherwise, it launches a new mpv instance.

Use **mpv** and **yt-dlp** to play video and music from the websites.

Please use it with userscript:

[![play-with-mpv][badges-play-with-mpv]][play-with-mpv-enhanced]

*Note: This mod is compatible with the [Play with mpv Enhanced](https://greasyfork.org/en/scripts/542145-play-with-mpv-enhanced) userscript. The original Play with mpv userscript is not compatible with queueing abilities.*

## Key Features

### Playlist Detection & Prefetching
When a URL is passed, the handler uses `yt-dlp` to check if it's a playlist. If it is, it fetches the direct, playable URLs for the videos. This is a crucial pre-fetching step that avoids buffering, as mpv receives a direct link to the media, not just a webpage URL.

### Queueing via IPC Socket
The handler then sends these direct URLs to the running mpv instance via its IPC socket, using the `loadfile append` command to build the queue seamlessly in the background.

### Interactive Control
To make it user-friendly, if a playlist is detected, the handler shows a `zenity` dialog asking the user how many videos to queue (with '0' for all). It has a 10-second timeout that defaults to queueing the entire playlist. The user can also choose to play only the first video, ignoring the rest of the playlist.

## Protocol

![](share/proto.png)

### Scheme

- `mpv`: Run mpv-handler without console window
- `mpv-debug`: Run mpv-handler with console window to view outputs and errors

### Plugins

- `play`: Use mpv player to play video

### Encoded Data

Use [URL-safe base64][rfc-base64-url] to encode the URL or TITLE.

Replace `/` to `_`, `+` to `-` and remove padding `=`.

Example (JavaScript):

```javascript
let data = btoa("https://www.youtube.com/watch?v=Ggkn2f5e-IU");
let safe = data.replace(/\//g, "_").replace(/\+/g, "-").replace(/\=/g, "");
```

### Parameters (Optional)

```
cookies = [ www.domain.com.txt ]
profile = [ default, low-latency, etc... ]
quality = [ 2160p, 1440p, 1080p, 720p, 480p, 360p ]
v_codec = [ av01, vp9, h265, h264 ]
v_title = [ Encoded Title ]
subfile = [ Encoded URL ]
startat = [ Seconds (float) ]
enqueue = [ true, false ]
    *   `true`: Forces the video to be enqueued to an existing mpv instance. If no instance is running, it will fail.
    *   `false`: Forces a new mpv instance to be opened, even if one is already running.
    *   If omitted, the handler will automatically detect if an mpv instance is running and enqueue if possible, otherwise it will open a new instance.
```

## Building from Source

To build the `mpv-handler` from source, you will need to have Rust and Cargo installed. If you don't have them, you can install them using `rustup`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Once Rust and Cargo are installed, navigate to the project's root directory and run the following command to build the release version:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/mpv-handler`.

## Installation

After building from source, follow these steps to install `mpv-handler` on your Linux system:

1.  **Copy the `mpv-handler` binary** to your local bin directory:
    ```bash
    cp target/release/mpv-handler ~/.local/bin/
    ```

2.  **Copy the desktop files** for application integration:
    ```bash
    cp share/linux/mpv-handler.desktop ~/.local/share/applications/
    cp share/linux/mpv-handler-debug.desktop ~/.local/share/applications/
    ```

3.  **Set executable permission** for the binary:
    ```bash
    chmod +x ~/.local/bin/mpv-handler
    ```

4.  **Register xdg-mime** to associate the `mpv://` and `mpv-debug://` schemes with the handler:
    ```bash
    xdg-mime default mpv-handler.desktop x-scheme-handler/mpv
    xdg-mime default mpv-handler-debug.desktop x-scheme-handler/mpv-debug
    ```

5.  **Add `~/.local/bin` to your environment variable PATH** (if not already present) to ensure the system can find the `mpv-handler` binary. You can add this line to your `~/.bashrc`, `~/.zshrc`, or equivalent shell configuration file:
    ```bash
    export PATH="$HOME/.local/bin:$PATH"
    ```
    After adding, remember to source your shell configuration file (e.g., `source ~/.bashrc`) or restart your terminal.

6.  **(Optional) Configure `mpv-handler` with `config.toml`**:
    You can copy the example `config.toml` to `~/.config/mpv-handler/config.toml` and configure paths for `mpv` and `yt-dlp`, or set a proxy. This file is located at:
    *   `~/.config/mpv-handler/config.toml`

    Example `config.toml` (for `mpv-handler`):

    ```toml
    mpv = "/usr/bin/mpv"
    # Optional, Type: String
    # The path of mpv executable binary
    # Default value:
    # - Linux: mpv

    ytdl = "/usr/bin/yt-dlp"
    # Optional, Type: String
    # The path of yt-dlp executable binary

    proxy = "http://example.com:8080"
    # Optional, Type: String
    # HTTP(S) proxy server address
    ```

## Configuration

To enable the queueing functionality and ensure the best experience, you need to configure mpv.

1.  **Create or edit your `mpv.conf` file**:
    *   This file is usually located at `~/.config/mpv/mpv.conf`.

2.  **Add the following recommended settings to `mpv.conf`**:

    ```
    # --- IPC and Handler Configuration ---
    # Enables the socket for the handler to communicate with mpv (required).
    input-ipc-server=/tmp/mpvsocket

    # --- YouTube/Streaming Quality ---
    # The handler will read this format to pre-fetch the correct quality.
    ytdl-format=bestvideo[height<=?1920][fps<=?30][vcodec^=avc]+bestaudio/best

    # Ensures mpv's internal hook uses the correct yt-dlp binary
    script-opts=ytdl_hook-ytdl_path=/usr/local/bin/yt-dlp

    # --- Caching and Prefetching ---
    # These options ensure smooth playback and minimize buffering for streamed content.
    prefetch-playlist=yes
    cache=yes
    demuxer-readahead-secs=300
    demuxer-max-bytes=500M
    ```

    *Note*: The `mpv-handler` uses `/tmp/mpvsocket` by default. Ensure this matches the path in your `mpv.conf`.


**(Optional) Install Zenity**: For the interactive playlist dialog, you need to have `zenity` installed. If it's not found, the dialog is skipped, and the handler will default to loading the entire playlist.

[rfc-base64-url]: https://datatracker.ietf.org/doc/html/rfc4648#section-5
[badges-aur-git]: https://img.shields.io/aur/version/mpv-handler-git?style=for-the-badge&logo=archlinux&label=mpv-handler-git
[badges-aur]: https://img.shields.io/aur/version/mpv-handler?style=for-the-badge&logo=archlinux&label=mpv-handler
[badges-play-with-mpv]: https://img.shields.io/greasyfork/v/416271?style=for-the-badge&logo=greasyfork&label=play-with-mpv-enhanced
[download-aur-git]: https://aur.archlinux.org/packages/mpv-handler-git/
[download-aur]: https://aur.archlinux.org/packages/mpv-handler/
[download-linux]: https://github.com/akiirui/mpv-handler/releases/latest/download/mpv-handler-linux-amd64.zip


[play-with-mpv-enhanced]: https://greasyfork.org/en/scripts/542145-play-with-mpv-enhanced
[linuxuprising]: https://www.linuxuprising.com/2021/07/open-youtube-and-more-videos-from-your.html
