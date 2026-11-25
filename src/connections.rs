//! Wi-Fi and Bluetooth state and control for the Deck tiles.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Connections {
    pub wifi: bool,
    pub bluetooth: bool,
    pub network: Option<String>,
}

async fn output(bin: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(bin).args(args).output().await.ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Read live radio state from NetworkManager and BlueZ.
pub async fn state() -> Connections {
    let wifi = output("nmcli", &["radio", "wifi"]).await
        .map(|s| s.eq_ignore_ascii_case("enabled")).unwrap_or(false);

    let bluetooth = output("bluetoothctl", &["--timeout", "3", "show"]).await
        .and_then(|s| s.lines().find(|l| l.contains("Powered:"))
            .map(|l| l.to_lowercase().contains("yes")))
        .unwrap_or(false);

    let network = output("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"]).await
        .and_then(|s| s.lines().find(|l| l.starts_with("yes:"))
            .and_then(|l| l.split_once(':')).map(|(_, ssid)| ssid.to_string()))
        .filter(|s| !s.is_empty());

    Connections { wifi, bluetooth, network }
}

/// Enable or disable one radio. Returns false when the tool is unavailable.
pub async fn set(target: &str, enabled: bool) -> bool {
    match target {
        "wifi" => output("nmcli", &["radio", "wifi", if enabled { "on" } else { "off" }]).await.is_some(),
        "bluetooth" => output("bluetoothctl", &["power", if enabled { "on" } else { "off" }]).await.is_some(),
        _ => false,
    }
}
