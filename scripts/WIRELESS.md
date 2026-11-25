# Ryoku Box — connection (current)

## Normal use: nothing to do

Install the APK, open **Ryoku Box**, and it pairs itself.

1. The app contacts the PC at the LAN address baked into the APK (an IP only —
   not a secret).
2. It calls `GET /api/pair-code` and receives a short-lived one-time code.
3. It exchanges that code at `/api/pair` for a permanent bearer token.
4. The token is stored in the app's own WebView storage. Later launches skip
   pairing entirely.

Requirements: phone and PC on the same Wi‑Fi, and the PC running
`ryokubox.service` (the mDNS service is optional and not needed for pairing).

No cable, no USB debugging, no camera, no pairing code and no firewall command.

## If automatic pairing fails

The app shows its own setup screen with three choices:

- **SCAN QR (camera)** — scan the Wi‑Fi QR on `http://127.0.0.1:8875/console`.
- **PAIR** — type the one-time code printed under that QR.
- **USE TOKEN** — paste the permanent token from the console.

The console is loopback-only and returns 403 from a phone. Open it on the PC.

## Rotating the token or changing networks

- **Token rotated:** the phone's saved token is stale. Clear the app's data or
  use `USE TOKEN` with the new token from the console.
- **PC LAN IP changed:** rebuild so the baked fallback address is current:

```sh
cd ~/ryokubox && ./scripts/connect.sh setup
```

## Check state

```sh
cd ~/ryokubox && ./scripts/connect.sh status
```

Shows agent health, LAN address, mDNS state, firewall rule and ADB devices. ADB
is only used to install/rebuild the APK — the deck does not use it.
