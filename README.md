<h1 align="center">Discord Tauri — kernoeb fork</h1>

<p align="center">
    A native-feeling Discord wrapper built on <a href="https://github.com/tauri-apps/tao">tao</a>, <a href="https://github.com/tauri-apps/wry">wry</a>, <a href="https://github.com/tauri-apps/tray-icon">tray-icon</a> and <a href="https://github.com/tauri-apps/muda">muda</a>.<br>
    Fork of <a href="https://github.com/eye-wave/discord-tauri">eye-wave/discord-tauri</a>, focused on a polished macOS experience.
</p>

---

## What this fork adds

- **macOS app bundle workflow** — `make bundle-macos` produces a signed `.app` with a stable code-signing identity so TCC permissions (mic/camera) survive rebuilds.
- **Rounded-square app icon** — redesigned in the macOS Big Sur style with proper safe-area inset, regenerated at all required `.icns` and `.ico` sizes.
- **Menu bar tray badge** — a high-resolution template silhouette that auto-tints to the menu bar; a non-template white-glyph variant with a red dot is shown when there are unread mentions or messages.
- **macOS dock badge** — `NSDockTile.badgeLabel` reflects Discord's unread count (capped at `99+`), or `•` for mentions without a count.
- **Title-driven badge state** — a `MutationObserver` watches `document.title` and forwards the state over IPC, so the native UI updates without polling.
- **Tracking blocker** — fetch / XHR / `sendBeacon` requests to Discord analytics (`/science`, `/track`) and Sentry endpoints are short-circuited with a 204.
- **No white flash** — bootstrap HTML, layer/contentView, and `WKWebView` background are all painted black before Discord's CSS loads, on macOS, Linux and Windows.
- **macOS title bar drag** — the top 32 px (excluding the traffic-light area) drags the window, with grab/grabbing cursor feedback.
- **macOS app menu** — standard `App / Edit / Window` submenus with the usual key equivalents (cut/copy/paste, undo/redo, hide, quit, …).
- **DNS prewarming** — `discord.com`, `gateway.discord.gg`, `cdn.discordapp.com` and `media.discordapp.net` are resolved on a background thread before the WebView is created, shaving the first round-trip off cold start.
- **Cached icon assets** — the tray RGBA buffers are decoded and post-processed once via `OnceLock`; badge transitions skip PNG decode and pixel work.
- **Debug-only test menu** — in `cargo run` (debug) builds, the tray menu exposes `Test: badge dot / 5 / 99+ / clear / resume auto` to exercise the badge code without waiting for real Discord notifications.
- **Larger default window** — 1280×800 instead of 800×600.

## Stack

| Component   | Crate                                                              |
| ----------- | ------------------------------------------------------------------ |
| Window      | [`tao`](https://github.com/tauri-apps/tao) 0.35                    |
| Web view    | [`wry`](https://github.com/tauri-apps/wry) 0.55                    |
| Tray        | [`tray-icon`](https://github.com/tauri-apps/tray-icon) 0.24        |
| Menus       | [`muda`](https://github.com/tauri-apps/muda) 0.19                  |
| Image       | [`image`](https://github.com/image-rs/image) 0.25                  |
| macOS API   | [`objc2`](https://github.com/madsmtm/objc2) + `objc2-app-kit` etc. |

The Tauri framework itself is not used — only its lower-level building blocks.

## Building

The release profile uses cargo features that require the **nightly** toolchain (`profile-rustflags`, `trim-paths`).

```sh
# debug run (with devtools and the test menu)
cargo +nightly run

# release binary
cargo +nightly build --release
```

### macOS bundle

```sh
make bundle-macos          # builds release + assembles "Discord Tauri.app"
make signing-cert          # one-time: create a stable self-signed identity
```

`make signing-cert` creates a Keychain certificate named `discord-tauri local` that the bundle script picks up automatically. With it in place, rebuilds preserve TCC permissions (camera, microphone, …) instead of re-prompting on every run. Without it, the bundle is ad-hoc signed and permissions reset per rebuild.

The output is `target/release/Discord Tauri.app`. Move it to `/Applications` and launch as usual.

### Linux

```sh
cargo +nightly build --release
```

The webview defaults to the Wayland GDK backend (`GDK_BACKEND=wayland`).

## Configuration & data locations (macOS)

| Path                                                  | What lives there                                         |
| ----------------------------------------------------- | -------------------------------------------------------- |
| `~/Library/Caches/discord.tauri/WebKit/NetworkCache`  | HTTP cache (Cache-Control / ETag), like Safari           |
| `~/Library/Caches/discord.tauri/WebKit/CacheStorage`  | Service-worker Cache API                                 |
| `~/Library/WebKit/discord.tauri/WebsiteData/Default`  | cookies                                                  |
| `~/Library/WebKit/discord.tauri/WebsiteData/LocalStorage` | localStorage                                         |
| `~/Library/WebKit/discord.tauri/WebsiteData/IndexedDB`    | IndexedDB                                            |

Persistence is enabled by default (wry uses `WKWebsiteDataStore::defaultDataStore`); the WebView is **not** in incognito mode.

## Status

- [x] Wraps Discord (web client at `discord.com/app`)
- [x] System tray with menu bar template icon
- [x] Tray badge + macOS dock badge for unread / mention state
- [x] Tracking blocker (Discord science / Sentry)
- [x] No-flash dark loading (macOS, Linux, Windows)
- [x] macOS native title bar drag
- [x] macOS app menu (services, hide, edit, window, quit)
- [x] Linux Wayland support
- [ ] Rich Presence
- [ ] Native notifications (badge only — no banner / sound today)
- [ ] Push to talk
- [ ] File drop

## ToS notice

Wrapping the Discord web client is technically a violation of Discord's Terms of Service, like any third-party client. Use at your own risk.

## License

GPLv3, inherited from upstream. Original authors: `DrPuc`, `eyewave`. macOS work in this fork by [`kernoeb`](https://github.com/kernoeb).
