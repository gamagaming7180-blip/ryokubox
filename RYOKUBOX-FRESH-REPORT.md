# Ryoku Box — fresh build report

Date: 2026-11-25 · Version: 1.0.0 · GPL-3.0-or-later

## 1. Clean slate

The old `ryokubox` tree at `/home/gamanotsuffering/ryokubox` was wiped: its
`Cargo.toml`, `src/`, `panel/`, `systemd/` and all build artifacts were deleted,
keeping only `.git`. Nothing from the old source or old CSS was read, patched or
carried forward. The replacement is a brand-new single Rust binary plus a static
Lithic panel.

No stale APK was installed on either connected device (`4555f3e4` USB and
`192.168.0.163:37847` Wi-Fi), so nothing needed uninstalling.

The real Lithic specs were cloned from `Ryoku-dev/ryoku` branch `hyperion`. The
web library lives at `docs/lithic/library/web/` on that branch (the paths in the
brief were one directory short). All five referenced files were read in full,
plus the library's imports (`fonts.css`, `foundation.css`, `controls.css`,
`segmented.css`, `lists.css`, `overlays.css`) and `README.md`. The clone was
deleted before the build, as required.

## 2. What was built

Backend, `src/`:

| File | Responsibility |
|---|---|
| `main.rs` | axum router on 8875, `ConnectInfo` loopback guards, pairing, SSE, all endpoints, 800 ms poll loop |
| `auth.rs` | permanent token, `subtle::ConstantTimeEq` bearer/pair checks, 120 s single-use codes |
| `wallpaper.rs` | swww / ryogami / pointer detection, `~` expansion, ffmpeg 1920px stills, Matugen→Lithic role mapping |
| `shortcuts.rs` | `id`/`label`/`argv`/`approved` model, atomic temp-file + rename save, direct `Command::new(argv[0]).args(...)` |

Frontend, `panel/`:

| Path | Responsibility |
|---|---|
| `index.html` | cut top bar, three horizontal snap pages, hold bar, media stage |
| `css/lithic.css` + imports | **real Lithic web library**, GPL-3.0 headers retained |
| `css/app.css` | panel skin built only from Lithic tokens and cut geometry |
| `css/fonts.css`, `fonts/` | Geologica, Martian Mono and Material Symbols Sharp bundled locally |
| `js/app.js` | authenticated `fetch()` SSE, Blob preload + crossfade, tiles, sliders, hold gestures |
| `manifest.json`, `sw.js`, `icons/icon.svg` | fullscreen landscape PWA |

`systemd/ryokubox.service` and `systemd/ryokubox-mdns.service` were installed to
`~/.config/systemd/user/`, enabled, and the main unit is **active**.
`ryokubox.desktop` is installed to `~/.local/share/applications/`, so the panel
appears in the app launcher (Super+Space) as "Ryoku Box" and opens in Firefox.

The panel is a web surface on port 8875; it does not open a window by itself.
`css/app.css` also forces `[hidden] { display: none !important; }` so the pairing
gate cannot sit over the deck (author `display:grid` was overriding `hidden`).

### Lithic fidelity

- Every card, bar, tile, drawer, button, badge, gate and bottom bar uses real
  Lithic `l-cut` clip-path geometry (45° chamfers on `cut.token` 3px,
  `cut.small` 6px, `cut.mid` 7px, `cut.overlay` 8px, `cut.large` 16px). No
  rounded pill chrome anywhere. The album art is the only intentionally round-
  cornered surface (content, not chrome), as `ui-ux.md` permits.
- Roles only, no hardcoded hex in the panel skin: `--lithic-*` plane, ink,
  selection, action, mineral and overlay variables. The signature table is the
  fallback; SSE overwrites it live with the wallpaper palette.
- Space run `4 / 8 / 13 / 21 / 34 / 55px` via `--lithic-space-s1..s6`.
- OutCubic `[0.2, 0, 0, 1]` (`--lithic-motion-curve`) throughout: press 90 ms,
  navigate 200 ms, select 240 ms, field 320 ms. No springs.
- Fonts: `Geologica` (UI), `Martian Mono` (clock, vitals, labels, numbers),
  `Material Symbols Sharp` (icons) — all local, offline over USB.
- Wallpaper: `#wp-a`/`#wp-b` dual layer, matte scrim and CSS grain, Blob
  preloaded before a 200 ms crossfade. On connect and on every detected
  wallpaper/palette change the agent pushes `{version, wallpaper_url, palette}`.
- Pages use horizontal `scroll-snap`, vertical scroll per page, 48px minimum
  touch targets, transparent tap highlight and `overscroll-behavior: contain`.

### Security behaviour

- Auto/Wi-Fi binds `0.0.0.0:8875`; `--mode usb` binds `127.0.0.1:8875`.
  Both verified with `ss`.
- `/console`, `/console/qr.svg`, token rotation and approval are loopback-only
  via `ConnectInfo<SocketAddr>`; LAN gets 403.
- QR contains only `http://<lan-ip>:8875/?pair=<code>`, never the token. Codes
  expire in 120 s and are single-use; `subtle` constant-time comparison.
- All other `/api/*` require `Authorization: Bearer <token>`; the permanent
  token is persisted `0600` at `~/.config/ryokubox/token` and survives restarts.
- Shortcut edits from LAN are held unapproved; `/api/run/{id}` refuses them.

## 3. Verification

`cargo check` — clean, no warnings.

`cargo test` — **13 passed, 0 failed**. Covers constant-time auth accept/reject,
bearer parsing, loopback guard, single-use/expiring pairing, atomic shortcut
save and ordering, unapproved shortcut gating, Matugen→Lithic role mapping,
tilde expansion, video extension detection, art-host allow-list, volume parsing
and SSE event serialization.

`cargo build --release` — success; 2.3 MB binary installed at `~/.local/bin/ryokubox`.

Live curl results (service running, LAN `192.168.0.176`):

| Check | Result |
|---|---|
| `GET /console` loopback / LAN | `200` / `403` |
| `GET /console/qr.svg` loopback / LAN | `200 image/svg+xml` / `403` |
| `POST /api/pair` with fresh code / reused code | `200 {"token":...}` / `401` |
| `GET /api/audio` no bearer / valid bearer | `401` / `200` |
| `GET /api/wallpaper` | `200 image/png`, 3,476,989 bytes (live 2560×1440 wallpaper from ryogami) |
| `GET /api/events` | streams `{version:1, wallpaper_url:"/api/wallpaper?v=1", palette:{...source:"matugen"}}` with live gold/primary, jasper/error, sel/secondaryContainer, ink/onSurface and surface tiers |
| `GET /api/audio` | volume 70, muted false, sink "Ryzen HD Audio Controller Speaker", player "spotify", 2 sinks listed |
| `GET /api/vitals` | cpu 12.8, ram 63%, uptime; absent temp/battery sensors omitted as specified |
| LAN shortcut PUT → run | `403` until approved in `/console`; `200` after |
| `POST /api/token/rotate` LAN / loopback | `403` / `200`; old token then `401`, new token `200` |
| Token across service restart | still valid |
| USB mode bind | `127.0.0.1:8875`; LAN connection refused |
| Static assets `/`, CSS, JS, fonts, manifest, sw, icon | all `200` |

Note: album-art URL was `null` from Spotify's MPRIS session during the test, so
the art proxy itself was not exercised with a live URL; the host allow-list is
unit-tested and arbitrary hosts are rejected with 403.

## 4. Open it on the phone

First open the console on the computer: <http://127.0.0.1:8875/console>. It shows
**two QR codes** — USB and Wi-Fi — plus the one-time pairing URLs and the
permanent token. Both QRs carry the same single-use code.

Note: on this machine the phone can ping the PC but the local firewall drops
inbound TCP 8875, so the Wi-Fi address is not reachable from the phone. Use the
USB path below. The reverse tunnel works even over the phone's Wi-Fi ADB
connection, so `adb reverse` is the reliable route in both cases.

**USB (recommended, works offline):**

```sh
adb reverse tcp:8875 tcp:8875
```

Then scan the **USB** QR in the console, or open the USB pairing URL on the phone:

```text
http://127.0.0.1:8875/?pair=<code-from-console>
```

The phone must be unlocked and the browser foregrounded; a locked phone renders
a black screen even though the page has loaded and paired.

**Wi-Fi (same network):**

Scan the QR in the console, or open:

```text
http://192.168.0.176:8875/?pair=<code-from-console>
```

The current LAN IP is printed in the console and in the service log; use that if
`192.168.0.176` has changed. After the first pair the token is stored in the
browser, so subsequent opens need no code. Add to the home screen from the
browser menu for the fullscreen landscape PWA.

## 5. Android app

`android/` contains a real, signed APK built with the Android SDK command-line
tools (no Gradle required):

```sh
./android/build.sh          # produces android/RyokuBox.apk
adb install -r android/RyokuBox.apk
```

`MainActivity.java` is a fullscreen landscape WebView with its own connect
screen (host field, one-time pairing code, persistent host), so the token lives
in the app's WebView storage and it reconnects on launch. The `⋯` button in the
bottom-right reopens the connect screen. It uses the ADB reverse tunnel by
default (`127.0.0.1:8875`) and accepts a LAN host too.

Verified on the connected Xiaomi M2101K6P (Android 17 / SDK 37): installed,
launched, paired, and the live panel rendered with wallpaper, clock, CPU/RAM,
now-playing and the USB connection chip. The APK is 13 KB.

## 6. See / undo

- See: open `http://127.0.0.1:8875/` on the computer; watch `/api/events` for
  palette changes and change the desktop wallpaper to trigger a crossfade.
- Undo installation: `systemctl --user disable --now ryokubox.service ryokubox-mdns.service`,
  then remove `~/.local/bin/ryokubox`, `~/.config/systemd/user/ryokubox*.service`
  and `~/.config/ryokubox/`. The repo itself is left intact.

## 7. Follow-up UI changes (same day)

- Removed all vertical scrolling: each slide is `height:100dvh; overflow:hidden`,
  sized with `clamp()`, so Home / Deck / Media / Settings fit the phone screen.
- Added translucent, blurred cut chrome (`--rb-glass`, `--rb-glass-strong`) to
  the bar, cards, tiles, chip, badges, bottom bar and panels.
- Rebuilt Media: HD album-art corner, centred seek + volume + player/output
  badges, transport bottom-right. Fixed MPRIS: `mpris:artUrl` is now read (the
  old `artUrl` field was always empty) and Spotify is preferred over
  kdeconnect. Verified live: `i.scdn.co` art proxy returned `200 image/jpeg`.
- Home now uses the empty area for a high-definition Now Playing banner backed
  by the real album art (wallpaper as fallback) with title, artist, player and
  sink.
- Added a fourth Settings slide with mode, contrast, reduce-motion, connection
  info, token rotation, wallpaper refresh and shortcut add/remove.
- Navigation is touch-sized: four 44px+ tabs plus one-slide-per-swipe gestures
  that ignore control touches; arrow keys and `#0`–`#3` deep links also work.
- Volume now spans the full 0–150% range on both sliders; verified API values
  `0 / 100 / 150` map to `wpctl` `0.00 / 1.00 / 1.50`. The live poll no longer
  fights an in-progress drag.

## 8. Radio tiles, Media cover and Settings rework

- Added `src/connections.rs` with `GET /api/connections` and
  `POST /api/connections/:target`. Wi-Fi uses `nmcli radio wifi` (and reports
  the active SSID), Bluetooth uses `bluetoothctl power`/`show`. The Deck tiles
  are now real controls: tapping Wi-Fi or Bluetooth toggles the radio and the
  tile shows live state (`Aryans room 5GHz` / `ON` / `OFF`). Mute still toggles
  the audio sink. Verified on the machine: BT off then on returned
  `bluetooth:false` then `true`; Wi-Fi stayed up with its SSID.
- Media cover enlarged to `clamp(260px, 38vh, 420px)` with a soft art shadow and
  larger title/artist type; transport, seek, volume and player/output badges
  rebalanced to the right of the cover.
- Settings rebuilt as an index rail of four numbered sections (Appearance,
  Connection, Shortcuts, About) with one cut pane shown at a time, instead of
  the earlier four-card grid.

`cargo test`: 13 passed. The Android app was force-reloaded and screenshots
confirmed the live Deck (SSID + BT state + big mini cover), the enlarged Media
cover with real Spotify art, and the new Settings index/pane layout.

## 9. Phone screen timeout and tile fix

The phone's `screen_off_timeout` was 30,000 ms (30 s). Set to 1,800,000 ms
(30 min) and `stay_on_while_plugged_in=7`, so the deck stays lit while charging.
Undo with:
```sh
adb -s 4555f3e4 shell settings put system screen_off_timeout 30000
adb -s 4555f3e4 shell settings put global stay_on_while_plugged_in 0
```
Also fixed the Deck Bluetooth tile: the tile id `bt` is now mapped to the API
target `bluetooth`. On-device tap verified: `bluetooth:true -> false -> true`
while Wi-Fi and its SSID stayed up.

## 10. Vitals, copy, spacing, dots and transport

- Vitals were frozen because a single-shot `sysinfo` CPU read returned a stale
  value. `audio::cpu_percent()` now computes CPU from a real `/proc/stat` delta
  over 180 ms. Verified three live samples: 15.6 / 16.8 / 15.0 while RAM moved.
- Removed the "AI agent" framing: the Home footer now reads `LINK ACTIVE · …`,
  the boot placeholder is `LINKING…`, the About text calls the app a
  PhoneBox-inspired Lithic deck, and "AGENT" became "SERVICE". This is a device
  deck, not an assistant.
- Added bottom padding to the Home footer so it no longer overlaps the System /
  Memory / Output pills.
- Removed the three-dot native overlay button from the Android app and rebuilt
  the APK. Its function now lives in Settings `05 / LINK & PAIRING` (host,
  optional pair code, Apply & connect); the settings index has five sections.
- Play/pause now reports real state: `AudioState.playing` comes from
  `playerctl status`, and every play/pause control swaps `play_arrow` / `pause`
  with an optimistic tap update. Verified on the phone: Playing -> Paused ->
  Playing and the icon followed.

## 11. Deck-native touches

- The Android app now sets `FLAG_KEEP_SCREEN_ON`: the deck never sleeps while
  it is in front, on top of the 30-minute system timeout.
- Physical phone volume keys are captured in `dispatchKeyEvent` and bridged to
  `window.rbAdjustVolume()` in the panel, so they raise/lower the computer's
  sink (0–150%) instead of the phone's media volume. Verified over Wi-Fi ADB:
  `0.55 -> 0.56 (VOL+) -> 0.51 (VOL-)`.

## 12. Volume reset / "illegal increment" root cause

The laptop's ALSA Speaker control is reasserted to 41/87 (~26%) about half a
second after any sink-volume change, so raising the volume appeared to be
blocked. This happens for `wpctl`, `pactl`, `amixer` and the app alike; it is a
desktop-side policy, not a Ryoku Box bug. PipeWire also mishandles sink values
above unity on this box, so the reliable ceiling is 100%.

`audio.rs` now controls **per-stream volume** via
`pactl set-sink-input-volume <index> <n>%`, which persists, and reports the
active stream's level in `/api/audio`. The sink is still set best-effort.
Sliders and hardware volume keys are capped at 100%. Verified through the API:
10 / 40 / 70 / 100 / 55 all read back correctly after a 2-second settle.

## 13. Wireless connection paths

Added `scripts/connect.sh` (`usb|wifi|lan|status`) and
`scripts/WIRELESS.md` with step-by-step joining instructions.

- **Wi-Fi ADB (recommended, working now):** phone wireless debugging at
  `192.168.0.163:37847`, `adb reverse tcp:8875 tcp:8875`, phone opens
  `http://127.0.0.1:8875/`; verified `200` from the phone shell. No firewall
  change needed.
- **Direct LAN:** PC is `192.168.0.176:8875`, but the firewall drops inbound
  8875 from the phone (`000`). Instructions cover `sudo ufw allow 8875/tcp`,
  then host `192.168.0.176:8875` in Settings 05.
- **USB:** always available with `./scripts/connect.sh usb` and host
  `127.0.0.1:8875`.
- **Bluetooth:** radio control works, but no data link (no rfcomm/pand/bt-network);
  documented as not implemented rather than pretending it works.

The printed pairing code was validated against `POST /api/pair` (`200`).
