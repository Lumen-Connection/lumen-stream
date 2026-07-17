<div align="center">

# 📥 Lumen Stream

**A lightweight, native media download hub that centralizes your downloads with high throughput, safe file handling, and zero tracking.**

Video and audio from every site `yt-dlp` supports, a resumable download queue, native PDF conversion, and a local library — all through a single, clean, extremely lightweight interface.

![Rust](https://img.shields.io/badge/Rust-2024-CE422B?logo=rust&logoColor=white)
![egui](https://img.shields.io/badge/egui%20%2F%20eframe-0.27-FF5722)
![Windows](https://img.shields.io/badge/Windows-x64-0078D6?logo=windows&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-local-003B57?logo=sqlite&logoColor=white)
![License](https://img.shields.io/badge/license-AGPL--3.0-blue)

</div>

## About

**Lumen Stream** is a native desktop app that centralizes media downloading in one efficient
place. You paste a link, pick a format, and it fetches the file through `yt-dlp` + `ffmpeg`,
organizing everything into a local library you fully control. No accounts, no telemetry — your
history stays on your machine, in a local SQLite database.

It is the community "media" project of the [Lumen Connection](https://lumenconnection.com.br/)
ecosystem, built to stop the abusive monetization of media download hubs by offering a free, open,
tracking-free alternative. Written in **Rust** with an immediate-mode UI for native performance and
low memory use.

## Features

### Downloading
- **Video and audio** from every source `yt-dlp` supports, with format and quality selection
- **Subtitles**, playlists, and **batch** downloads from a list of links
- **Spotify metadata** resolved automatically (title/artist) for cleaner filenames
- **Live stream recording** straight to disk

### Queue and organization
- **Resumable queue** with rate limiting, concurrent fragments, and automatic retry
- **Automatic organization** into folders, with optional copy to a cloud folder
- **Tracked folders** you can open, rename, and manage from inside the app

### Conversion
- **Native PDF conversion** for documents and spreadsheets — no LibreOffice required
- Media conversions handled by `ffmpeg` under the hood

### Library
- **Gallery** of everything you've downloaded, with thumbnails
- **Mini-player** to preview audio without leaving the app
- **Tag editing**, favorites, statistics, and audio transcription (Whisper)
- **Safe deletion** — files go to the system Recycle Bin (recoverable) before anything is erased

### Privacy and safety
- **Zero tracking.** No telemetry, analytics, or beacons of any kind
- **Everything local.** History and settings live in SQLite and JSON under your user directory
- Interface available in **Portuguese and English**

## Tech stack

- **Language:** Rust (2024 edition)
- **UI:** egui / eframe (immediate mode)
- **Storage:** SQLite (local database) + JSON config
- **Media:** `yt-dlp` + `ffmpeg`, fetched on demand on first use

## Requirements

- Windows 10 or later
- [Visual C++ for Visual Studio 2015–2022 x64](https://aka.ms/vs/17/release/vc_redist.x64.exe)
- [Rust toolchain](https://rustup.rs/) (only for building from source)

## Building

```sh
cargo build --release
```

The `lumen-stream.exe` binary lands in `target/release/`. On first run, the helper binaries
(`yt-dlp`, `ffmpeg`) are downloaded automatically as needed.

## License

Built by [Lumen Connection](https://lumenconnection.com.br/), distributed under the
[AGPL-3.0](LICENSE) license. Free, copyleft, and non-commercial software — designed to
decentralize digital control and protect the user.

<div align="center">

Made with Rust • ◆ Lumen Connection

</div>
