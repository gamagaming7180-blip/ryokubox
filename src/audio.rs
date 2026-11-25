//! System vitals, audio sinks, MPRIS media, a strict art proxy and session actions.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Ram {
    pub used: u64,
    pub total: u64,
    pub percent: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Vitals {
    pub cpu: f32,
    pub ram: Ram,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<u8>,
    pub uptime: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Sink {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AudioState {
    pub volume: u32,
    pub muted: bool,
    pub sink: String,
    pub sink_name: String,
    pub sinks: Vec<Sink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub art_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
}

fn first_matching(pattern: &str, file: &str) -> Option<String> {
    let base = pattern.rsplit_once('/').map(|(b, _)| b).unwrap_or("/sys/class");
    let leaf = pattern.rsplit('/').next().unwrap_or("*");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(base).ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.starts_with(leaf)).unwrap_or(false))
        .collect();
    entries.sort();
    for dir in entries {
        if let Ok(s) = std::fs::read_to_string(dir.join(file)) {
            let s = s.trim();
            if !s.is_empty() { return Some(s.to_string()); }
        }
    }
    None
}

fn cpu_ticks() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let nums: Vec<u64> = line.split_whitespace().skip(1)
        .filter_map(|v| v.parse::<u64>().ok()).collect();
    if nums.len() < 5 { return None; }
    let idle = nums[3] + nums[4]; // idle + iowait
    let total = nums.iter().sum::<u64>();
    Some((idle, total))
}

/// CPU load from a real /proc/stat delta; single-shot sysinfo reads go stale.
async fn cpu_percent() -> f32 {
    let (Some(a), Some(b)) = (cpu_ticks(), {
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        cpu_ticks()
    }) else { return 0.0 };
    let didle = b.0.saturating_sub(a.0);
    let dtotal = b.1.saturating_sub(a.1);
    if dtotal == 0 { 0.0 } else {
        ((1.0 - didle as f64 / dtotal as f64) * 100.0).max(0.0) as f32
    }
}

pub async fn vitals() -> Vitals {
    use sysinfo::System;
    let cpu = cpu_percent().await;
    let mut sys = System::new_all();
    sys.refresh_memory();
    let used = sys.used_memory();
    let total = sys.total_memory();
    let percent = if total == 0 { 0.0 } else { (used as f32 / total as f32) * 100.0 };
    let temp = first_matching("/sys/class/thermal/thermal_zone*", "temp")
        .and_then(|s| s.parse::<f32>().ok()).map(|v| v / 1000.0);
    let battery = first_matching("/sys/class/power_supply/BAT*", "capacity")
        .and_then(|s| s.parse::<u8>().ok());
    Vitals {
        cpu: (cpu * 10.0).round() / 10.0,
        ram: Ram { used, total, percent: (percent * 10.0).round() / 10.0 },
        temp: temp.map(|v| (v * 10.0).round() / 10.0),
        battery,
        uptime: sysinfo::System::uptime(),
    }
}

async fn wpctl(args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("wpctl").args(args).output().await.ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Same as `wpctl` but keeps the failure text so the caller can report it.
async fn wpctl_checked(args: &[&str]) -> Result<String, String> {
    let out = tokio::process::Command::new("wpctl").args(args).output().await
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "wpctl failed".to_string() } else { err })
    }
}

async fn pactl(args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("pactl").args(args).output().await.ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn parse_volume(s: &str) -> (u32, bool) {
    let muted = s.contains("[MUTED]");
    let pct = s.split_whitespace().nth(1).and_then(|v| v.parse::<f32>().ok())
        .map(|f| (f * 100.0).round() as u32).unwrap_or(0);
    (pct, muted)
}

fn parse_sinks(text: &str) -> Vec<Sink> {
    let mut sinks = Vec::new();
    let mut name = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Sink #") {
            if !name.is_empty() { sinks.push(Sink { name: name.clone(), description: name.clone() }); }
            name.clear();
        } else if let Some(v) = t.strip_prefix("Name: ") {
            name = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("Description: ") {
            if !name.is_empty() {
                sinks.push(Sink { name: name.clone(), description: v.trim().to_string() });
                name.clear();
            }
        }
    }
    if !name.is_empty() { sinks.push(Sink { name: name.clone(), description: name }); }
    sinks
}

/// PipeWire's sink volume on this box is reasserted by a desktop policy, but
/// per-stream volume persists and is what actually reaches the speakers.
async fn sink_input_indices() -> Vec<u32> {
    let text = command_output("pactl", &["list", "sink-inputs"]).await.unwrap_or_default();
    text.lines().filter_map(|l| {
        l.trim().strip_prefix("Sink Input #").and_then(|n| n.trim().parse::<u32>().ok())
    }).collect()
}

fn volume_percent(line: &str) -> Option<u32> {
    // "Volume: front-left: 26214 /  40% / -23.88 dB, ..."
    line.split('/').nth(1)
        .and_then(|part| part.trim().strip_suffix('%'))
        .and_then(|n| n.trim().parse::<u32>().ok())
}

async fn sink_input_volume() -> Option<u32> {
    let text = command_output("pactl", &["list", "sink-inputs"]).await?;
    text.lines().find(|l| l.contains("Volume:")).and_then(volume_percent)
}

async fn list_sinks() -> Vec<Sink> {
    if let Some(text) = pactl(&["list", "sinks"]).await {
        let parsed = parse_sinks(&text);
        if !parsed.is_empty() { return parsed; }
    }
    // Fallback: terse listing, description approximates the name.
    pactl(&["list", "short", "sinks"]).await.map(|text| {
        text.lines().filter_map(|line| {
            let mut parts = line.split_whitespace();
            let _idx = parts.next()?;
            let name = parts.next()?.to_string();
            Some(Sink { description: name.clone(), name })
        }).collect()
    }).unwrap_or_default()
}

async fn mpris() -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    // MPRIS exposes mpris:artUrl; {{artUrl}} is always empty on playerctl here.
    let fmt = "{{playerName}}\t{{title}}\t{{artist}}\t{{mpris:artUrl}}\t{{artUrl}}";
    let clean = |s: &str| {
        let s = s.trim();
        if s.is_empty() { None } else { Some(s.to_string()) }
    };
    let parse = |text: &str| {
        let parts: Vec<&str> = text.split('\t').collect();
        let player = parts.get(0).and_then(|s| clean(s));
        let title = parts.get(1).and_then(|s| clean(s));
        let artist = parts.get(2).and_then(|s| clean(s));
        let art = parts.get(3).and_then(|s| clean(s))
            .or_else(|| parts.get(4).and_then(|s| clean(s)));
        (player, title, artist, art)
    };

    // Prefer a real music player over kdeconnect's mirrored MPRIS interface.
    let mut players: Vec<String> = command_output("playerctl", &["-l"]).await
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    players.sort_by_key(|p| {
        let p = p.to_lowercase();
        if p.contains("spotify") || p.contains("ryotunes") { 0 }
        else if p.contains("kdeconnect") { 2 }
        else { 1 }
    });
    for player in &players {
        if let Some(text) = command_output("playerctl", &["-p", player, "metadata", "--format", fmt]).await {
            let parsed = parse(&text);
            if parsed.1.is_some() || parsed.0.is_some() { return parsed; }
        }
    }
    if let Some(text) = command_output("playerctl", &["metadata", "--format", fmt]).await {
        let parsed = parse(&text);
        if parsed.1.is_some() || parsed.0.is_some() { return parsed; }
    }
    let player = command_output("playerctl", &["-l"]).await
        .and_then(|s| s.lines().next().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty());
    (player, None, None, None)
}

async fn command_output(bin: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(bin).args(args).output().await.ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub async fn audio_state() -> AudioState {
    let (sink_volume, muted) = wpctl(&["get-volume", "@DEFAULT_AUDIO_SINK@"]).await
        .map(|s| parse_volume(&s)).unwrap_or((0, false));
    let volume = sink_input_volume().await.unwrap_or(sink_volume);
    let sinks = list_sinks().await;
    let sink_name = pactl(&["get-default-sink"]).await.unwrap_or_default();
    let sink = sinks.iter().find(|s| s.name == sink_name)
        .map(|s| s.description.clone()).unwrap_or_else(|| sink_name.clone());
    let (player, title, artist, art_url) = mpris().await;
    let playing = command_output("playerctl", &["status"]).await.map(|s| {
        let s = s.trim().to_lowercase();
        s.contains("playing")
    });
    AudioState { volume, muted, sink, sink_name, sinks, player, title, artist, art_url, playing }
}

/// Set volume (0..100) and/or mute. This box's PipeWire mishandles boosts
/// above unity (some values wrap to 0), so 100% is the reliable ceiling.
/// Returns (ok, error).
pub async fn set_audio(volume: Option<u32>, mute: Option<bool>, toggle: bool) -> (bool, Option<String>) {
    let mut ok = true;
    let mut error = None;
    if toggle {
        if wpctl_checked(&["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]).await.is_err() { ok = false; }
    } else if let Some(m) = mute {
        if wpctl_checked(&["set-mute", "@DEFAULT_AUDIO_SINK@", if m { "1" } else { "0" }]).await.is_err() { ok = false; }
    }
    if let Some(v) = volume {
        let requested = v.min(100);
        let pct = format!("{requested}%");
        // Stream volume is the persistent, audible control; set every output.
        for idx in sink_input_indices().await {
            let out = tokio::process::Command::new("pactl")
                .args(["set-sink-input-volume", &idx.to_string(), &pct])
                .output().await;
            match out {
                Ok(o) if o.status.success() => {},
                Ok(o) => { ok = false; error = Some(String::from_utf8_lossy(&o.stderr).trim().to_string()); }
                Err(e) => { ok = false; error = Some(e.to_string()); }
            }
        }
        // Best-effort sink update as well.
        let frac = format!("{:.2}", requested as f32 / 100.0);
        let _ = wpctl_checked(&["set-volume", "@DEFAULT_AUDIO_SINK@", &frac]).await;
    }
    (ok, error)
}

pub async fn media(action: &str) -> bool {
    let arg = match action {
        "play-pause" => "play-pause",
        "next" => "next",
        "prev" => "previous",
        "seek-back" => "position 10-",
        "seek-fwd" => "position 10+",
        _ => return false,
    };
    // seek actions carry a space: pass as two args
    let args: Vec<&str> = arg.split_whitespace().collect();
    command_output("playerctl", &args).await.is_some()
}

/// Album-art hosts we are willing to proxy. Everything else is rejected with 403.
const ART_HOST_ALLOWLIST: [&str; 12] = [
    "i.scdn.co", "scdn.co", "is1-ssl.mzstatic.com", "is2-ssl.mzstatic.com",
    "music.apple.com", "cover.art", "lastfm.img", "users-images.lastfm.com",
    "i.soundcloud.com", "artwork.youtube.com", "i.ytimg.com", "localhost",
];

pub fn art_url_allowed(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("file://") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        return PathBuf::from(rest).starts_with(home);
    }
    let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority)
        .split(':').next().unwrap_or(authority).to_lowercase();
    if host == "127.0.0.1" || host == "::1" { return true; }
    ART_HOST_ALLOWLIST.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Fetch an allow-listed image. Uses curl directly so no arbitrary host is contacted.
pub async fn fetch_art(url: &str) -> Option<(Vec<u8>, String)> {
    if !art_url_allowed(url) { return None; }
    if let Some(path) = url.strip_prefix("file://") {
        let bytes = tokio::fs::read(path).await.ok()?;
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        return Some((bytes, mime));
    }
    let tmp = std::env::temp_dir().join(format!("ryokubox-art-{}", crate::auth::random_hex(8)));
    let out = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "6", "-o"])
        .arg(&tmp)
        .args(["-w", "%{content_type}", url])
        .output().await.ok()?;
    if !out.status.success() { return None; }
    let mime = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let bytes = tokio::fs::read(&tmp).await.ok()?;
    let _ = tokio::fs::remove_file(&tmp).await;
    let mime = if mime.is_empty() { "image/jpeg".to_string() } else { mime };
    Some((bytes, mime))
}

pub async fn session(action: &str) -> bool {
    let argv: &[&str] = match action {
        "lock" => &["ryoku-shell", "lock"],
        "sleep" => &["ryoku-shell", "suspend"],
        "logout" => &["ryoku-shell", "session", "logout"],
        "restart" => &["ryoku-shell", "session", "reboot"],
        "poweroff" => &["ryoku-shell", "session", "poweroff"],
        _ => return false,
    };
    tokio::process::Command::new(argv[0]).args(&argv[1..]).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_parser_handles_mute_marker() {
        assert_eq!(parse_volume("Volume: 0.53"), (53, false));
        assert_eq!(parse_volume("Volume: 0.20 [MUTED]"), (20, true));
    }

    #[test]
    fn art_proxy_allowlist_is_strict() {
        assert!(art_url_allowed("https://i.scdn.co/image/abc"));
        assert!(art_url_allowed("http://127.0.0.1:8080/cover.jpg"));
        assert!(art_url_allowed("file:///home/gamanotsuffering/Pictures/a.jpg"));
        assert!(!art_url_allowed("https://evil.example.com/track.jpg"));
        assert!(!art_url_allowed("ftp://i.scdn.co/a.jpg"));
    }
}
