# Ryoku Box

A fullscreen Lithic companion deck for the Ryoku desktop, viewed on a phone.

The phone app auto-discovers the PC over Wi-Fi (mDNS `_ryokubox._tcp`), pairs with
its baked device key, and loads the panel served by the Rust agent. No cable, no
USB debugging, no pairing code, no firewall command.

## Layout

- `src/` — Rust/axum agent on port 8875 (vitals, audio, media, wallpaper, shortcuts, auth)
- `panel/` — HTML/CSS/JS deck (Geologica, Martian Mono, Material Symbols Sharp)
- `android/` — minimal Android WebView shell; `build.sh` bakes the current token and LAN IP
- `android/res/` — launcher icon, rasterized from the panel's own `icons/icon.svg` (same 力 mark)
- `scripts/connect.sh` — status / rebuild-and-install helper
- `scripts/mdns.sh` — publishes `_ryokubox._tcp` with the current IPv4 in TXT
- `systemd/` — user services for the agent and mDNS publisher

## Run on the PC

```sh
cargo build --release
cp systemd/*.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now ryokubox.service ryokubox-mdns.service
```

## Build the phone app

```sh
./android/build.sh
```

This reads `~/.config/ryokubox/token` and the current LAN IP, so the resulting
APK is tied to this PC. The built APK is deliberately **not** committed — it
contains that token.

## Notes

- Agent health / link status: `./scripts/connect.sh status`
- After rotating the token or changing networks: `./scripts/connect.sh setup`
- Licence: GPL-3.0-or-later
