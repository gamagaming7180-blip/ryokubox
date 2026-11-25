# Ryoku Box — connection

## Normal use: nothing to do

Open the app. It discovers the PC over Wi-Fi (mDNS `_ryokubox._tcp`), pairs with
its baked device key, and loads the deck. No cable, no USB debugging, no IP, no
pair code, no firewall command.

Requirements: phone and PC on the same Wi-Fi, and the PC running `ryokubox.service`
plus `ryokubox-mdns.service` (both are user systemd services and start on login).

## After the PC token is rotated or its LAN IP changes

One command on the PC, with the phone connected by cable once:

```sh
cd ~/ryokubox && ./scripts/connect.sh setup
```

It rebuilds the APK with the current token/LAN address, installs it, launches it,
and copies it to `~/Downloads/RyokuBox.apk`. After that, debugging can be turned
off again.

If you would rather not use adb at all, copy `~/Downloads/RyokuBox.apk` to the
phone (file manager over USB, KDE Connect, etc.) and install it manually.

## Check state

```sh
./scripts/connect.sh status
```

## Fallback

The old manual screen is still there if discovery fails: open the app, wait for
the connect screen, and type the host shown by `status`. It accepts the permanent
token from `http://127.0.0.1:8875/console` as the pairing code.
