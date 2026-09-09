# Local Cobalt companion

Lumen offers Auto (yt-dlp → Cobalt), yt-dlp, and Cobalt. Settings supply defaults;
individual downloads can override them, and queued jobs retain their selection.
Cobalt is installed on demand and listens only on loopback with a per-process API
key. No public instance, browser cookies, or external session service is used.

Auto attempts Cobalt once after an eligible yt-dlp extraction/network/dependency
failure. Filesystem errors, cancellation, content restrictions and finalization
errors do not switch engines. Queue network retries run after both attempts,
with the existing limit of two retries. Missing previews are allowed for direct
links; Cobalt does not depend on yt-dlp for metadata or finalization.

## First-release boundaries

Cobalt supports single media responses and batches of direct URLs. Playlist
expansion, Spotify/search, live recording, clips, subtitle downloads, detailed
format lists, and rate-limited requests require yt-dlp. Multi-item posts report
an unsupported-selection error. Pause/resume restarts Cobalt extraction and
transfer; temporary tunnel URLs are never persisted. Queued yt-dlp downloads
retain their own stable staging directory for resuming across restarts. Actual site availability
still depends on Cobalt and upstream platform behavior.

Both engines stage downloads separately and publish without overwriting an
existing destination. Cobalt transfers are streamed with unknown-size progress
where needed. Local FFmpeg preserves the selected output profile. History,
notifications and cloud copies happen only after finalization succeeds.

## Build and release

Windows x64 is the packaged target. Run `scripts/cobalt/package.ps1` with Node
22.23.2, pnpm 9.15.9, Git and native build tools installed on the **build machine**.
The script builds the pinned upstream commit, deploys production dependencies,
packages Node/FFmpeg, embeds source/notices, tests authentication and a controlled
extractor fixture, and writes `assets/cobalt/manifest.json`. Release CI builds
Lumen only after generating that manifest; the archive and application are
published together. The archive checksum is embedded in Lumen, not fetched from
an untrusted mutable manifest at runtime.

A source checkout intentionally has an `unreleased` manifest. It cannot download
an unverified companion. Generate the manifest locally or use the release build.
Old installed versions are retained for startup rollback. Repair restarts and
reinstalls the current version; do not invoke repair while downloads are active.

The only upstream adaptation is replacing `@imput/version-info/index.js` with
immutable version/commit exports: upstream otherwise requires `.git` at runtime.
The replacement source and original source archive are included in the bundle.
To upgrade: change the pinned commit/version and runtime in the packaging script,
review the API DTOs and service capabilities, run all gates, then release the
new application and checksum-bound archive together.

## Validation

`cargo test` runs deterministic unit/mock HTTP checks. Windows CI additionally
runs `cargo test cobalt_gate -- --ignored` after packaging, with
`LUMEN_COBALT_TEST_BUNDLE` and `LUMEN_TEST_FFMPEG` set by the package step. These
gates check actual supervisor startup/cleanup and an interactive plus queued
Cobalt download with a deliberately unusable yt-dlp executable, every audio/video
output profile, and descendant-process cleanup. Release is
blocked if any gate fails.

For an opt-in live check, select Cobalt and use a public URL you are authorized
to download from YouTube, SoundCloud, and a social-video service. Verify audio
and video outputs, cancel one transfer, then pause/restart a queue item. These
checks intentionally do not run against changing third-party content in CI.
Diagnostics stay local; API keys, signed media URLs and server error contexts
are not included in adapter error messages.
