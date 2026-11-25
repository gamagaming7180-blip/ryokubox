# Ryoku Box — build report (current state)

Date: 2026-11-25 · Version 1.0.0 · GPL-3.0-or-later

This report describes the **current** project. Earlier revisions used a
token-bearing APK and adb/mDNS pairing; those are no longer part of the design.

## 1. What it is

A single Rust/axum agent on the PC serves a Lithic-styled deck that an Android
phone displays fullscreen. The phone app is a thin WebView shell. The PC does
the work; the phone renders it.

- PC: Linux only (PipeWire/`wpctl`, `playerctl`, `nmcli`, `bluetoothctl`,
  `swww`, matugen, user systemd). `ryoku-shell` is used only for the five
  hold-to-activate session buttons.
- Phone: Android 7+ (minSdk 24), no root, no debugging for normal use.
- Panel: plain HTML/CSS/JS with local Geologica, Martian Mono and Material
  Symbols Sharp fonts.

## 2. Connection / security (final design)

- The APK contains **no permanent token** — only the PC's LAN IP as a fallback
  address. A leaked APK yields no credential.
- On first launch the app calls `GET /api/pair-code` at that address. The agent
  returns the same short-lived (120 s) one-time code shown in the console QR.
- The app redeems it at `POST /api/pair` and stores the resulting bearer token
  only in the phone's WebView storage.
- `/api/pair` no longer accepts the permanent token as a self-pair key, so the
  old baked-token APKs cannot pair.
- `/console`, its QR endpoints and token rotation remain loopback-only (403
  from the LAN).
- Caveat: `/api/pair-code` is unauthenticated on the LAN by design — any device
  on the same Wi‑Fi can pair itself. This is the zero-setup tradeoff.

Fallbacks kept in the app: in-app camera QR scanner (ZXing), one-time-code field,
and permanent-token paste field.

## 3. Layout

```
src/            axum agent: routes, auth, pairing, SSE, vitals, audio, wallpaper
panel/          deck: index.html, js/app.js, css/*.css, fonts/, icons/icon.svg
android/        WebView shell, QR scanner, build.sh, vendored ZXing, launcher icon
scripts/        connect.sh (status/setup), mdns.sh (optional publisher), WIRELESS.md
systemd/        ryokubox.service, ryokubox-mdns.service
```

## 4. Build and verify

```sh
cd ~/ryokubox
cargo test          # 13 passed
cargo build --release
./android/build.sh  # signed, token-free APK -> android/RyokuBox.apk
./scripts/connect.sh status
```

Verified on the Xiaomi M2101K6P and the Motorola edge 60 fusion: fresh install
of the token-free APK opened the deck with no input, showing `LINK ACTIVE ·
WI-FI` and the live wallpaper/palette. Token string is absent from the APK and
from the pushed diff.

## 5. Distribution

- Source: https://github.com/gamagaming7180-blip/ryokubox
- No APK is distributed and no APK is committed. The app has the builder's LAN
  IP baked in, so a prebuilt APK only works on that one network. Clone the repo
  on your PC, run the agent and build your own with `./android/build.sh`.
  The result is also copied to `~/Downloads/RyokuBox.apk` for sideloading.

## 6. Undo

`git revert <commit>` and push, or restore the backup at
`~/ryokubox-backups/ryokubox-FULL-2026-11-25.tar.gz`.
