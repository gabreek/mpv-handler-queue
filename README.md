

# mpv handler (queue mod)

A protocol handler for **mpv**, written in Rust, with an added queueing modification.

This modified version automatically detects if an mpv instance is already running with a socket open. If so, it enqueues the video to the existing instance. Otherwise, it launches a new mpv instance.

Use **mpv** and **yt-dlp** to play video and music from the websites.

Please use it with userscript:

[![play-with-mpv][badges-play-with-mpv]][play-with-mpv-enhanced]

*Note: This mod is compatible with the [Play with mpv Enhanced](https://greasyfork.org/en/scripts/542145-play-with-mpv-enhanced) userscript. The original Play with mpv userscript is not compatible with queueing abilities.*

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

## Configuration

To enable the queueing functionality, you need to configure mpv to listen on a socket. This allows `mpv-handler` to communicate with a running mpv instance.

1.  **Create or edit your `mpv.conf` file**:
    *   This file is usually located at `~/.config/mpv/mpv.conf`.

2.  **Add the following line to `mpv.conf`**:

    ```
    input-ipc-server=/tmp/mpvsocket
    ```

    *Note*: The `mpv-handler` uses `/tmp/mpvsocket` by default. Ensure this matches the path in your `mpv.conf`.

3.  **Optional `config.toml` for `mpv-handler`**:
    You can also create a `config.toml` file for `mpv-handler` to specify paths for `mpv` and `yt-dlp`, or to configure a proxy. This file is located at:
    *   `~/.config/mpv-handler/config.toml`

    Example `config.toml`:

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

[rfc-base64-url]: https://datatracker.ietf.org/doc/html/rfc4648#section-5
[badges-aur-git]: https://img.shields.io/aur/version/mpv-handler-git?style=for-the-badge&logo=archlinux&label=mpv-handler-git
[badges-aur]: https://img.shields.io/aur/version/mpv-handler?style=for-the-badge&logo=archlinux&label=mpv-handler
[badges-play-with-mpv]: https://img.shields.io/greasyfork/v/416271?style=for-the-badge&logo=greasyfork&label=play-with-mpv-enhanced
[download-aur-git]: https://aur.archlinux.org/packages/mpv-handler-git/
[download-aur]: https://aur.archlinux.org/packages/mpv-handler/
[download-linux]: https://github.com/akiirui/mpv-handler/releases/latest/download/mpv-handler-linux-amd64.zip


[play-with-mpv-enhanced]: https://greasyfork.org/en/scripts/542145-play-with-mpv-enhanced
[linuxuprising]: https://www.linuxuprising.com/2021/07/open-youtube-and-more-videos-from-your.html
