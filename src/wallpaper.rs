//! Live wallpaper detection, video still extraction and Matugen -> Lithic role mapping.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const IMAGE_EXT: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "avif"];
const VIDEO_EXT: [&str; 4] = ["mp4", "webm", "mkv", "mov"];

/// Runtime colour roles. Names are the Lithic roles the panel writes as CSS variables.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Palette {
    pub gold: String,
    pub on_gold: String,
    pub gold_ink: String,
    pub jasper: String,
    pub jasper_ink: String,
    pub sel: String,
    pub on_sel: String,
    pub ink: String,
    pub fg: String,
    pub fg2: String,
    pub fg3: String,
    pub ground: String,
    pub stone1: String,
    pub stone2: String,
    pub stone3: String,
    pub stone4: String,
    pub slab: String,
    pub field: String,
    pub vein: String,
    pub edge: String,
    pub scrim: String,
    pub source: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self::signature()
    }
}

/// Ryoku's Lithic signature dark table (tokens.json `signature.dark`), used when no
/// Matugen palette is installed yet, so the panel never paints blank chrome.
impl Palette {
    pub fn signature() -> Self {
        Self {
            gold: "#ce5e32".into(), on_gold: "#000000".into(), gold_ink: "#dc6a3e".into(),
            jasper: "#eb596c".into(), jasper_ink: "#fa6777".into(),
            sel: "#ce5e32".into(), on_sel: "#000000".into(),
            ink: "#cdc4ba".into(), fg: "#cdc4ba".into(), fg2: "#b0a9a0".into(), fg3: "#958f87".into(),
            ground: "#000000".into(), stone1: "#000000".into(), stone2: "#000000".into(),
            stone3: "#161514".into(), stone4: "#1b1918".into(), slab: "#000000".into(),
            field: "#23211f".into(), vein: "#46433f".into(), edge: "#66625e".into(),
            scrim: "#0000009e".into(), source: "signature".into(),
        }
    }
}

/// A fingerprint of the currently selected wallpaper file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub path: PathBuf,
    pub mtime: u128,
    pub size: u64,
}

/// The image actually served to the panel (original still or an extracted video frame).
#[derive(Clone, Debug)]
pub struct ServedImage {
    pub bytes: Vec<u8>,
    pub mime: String,
}

fn home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/root"))
}

fn fingerprint(path: &Path) -> Option<(u128, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some((mtime, meta.len()))
}

fn imagey(s: &str) -> bool {
    let lower = s.to_lowercase();
    let clean = lower.split('?').next().unwrap_or(&lower);
    IMAGE_EXT.iter().chain(VIDEO_EXT.iter()).any(|e| clean.ends_with(&format!(".{e}")))
}

fn expand_tilde(s: &str) -> String {
    if s == "~" {
        home().to_string_lossy().into_owned()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest).to_string_lossy().into_owned()
    } else {
        s.to_string()
    }
}

fn clean_path_string(s: &str) -> Option<PathBuf> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let s = s.strip_prefix("file://").unwrap_or(s);
    let p = PathBuf::from(expand_tilde(s));
    if p.is_file() { Some(p) } else { None }
}

/// Recursively find the first image-looking string anywhere in a JSON value.
fn first_path_in_json(v: &Value) -> Option<PathBuf> {
    match v {
        Value::String(s) if imagey(s) => clean_path_string(s),
        Value::Object(map) => {
            // Prefer obvious keys first, then walk everything.
            for key in ["wallpaper", "path", "image", "file", "current", "output"] {
                if let Some(hit) = map.get(key).and_then(first_path_in_json) { return Some(hit); }
            }
            map.values().find_map(first_path_in_json)
        }
        Value::Array(arr) => arr.iter().find_map(first_path_in_json),
        _ => None,
    }
}

async fn from_swww() -> Option<PathBuf> {
    let out = tokio::process::Command::new("swww").arg("query").output().await.ok()?;
    if !out.status.success() { return None; }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("image:") {
            let candidate = line[idx + 6..].trim();
            if let Some(p) = clean_path_string(candidate) { return Some(p); }
        }
    }
    None
}

fn from_ryogami() -> Option<PathBuf> {
    let p = home().join(".cache/ryogami/outputs.json");
    let raw = std::fs::read_to_string(p).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    first_path_in_json(&json)
}

fn from_pointers() -> Option<PathBuf> {
    let candidates = [
        home().join(".cache/ryoku/current_wallpaper"),
        home().join(".config/ryoku/current_wallpaper"),
    ];
    for c in candidates {
        if let Ok(raw) = std::fs::read_to_string(c) {
            if let Some(p) = clean_path_string(raw.lines().next().unwrap_or("")) { return Some(p); }
        }
    }
    None
}

/// Detect the active wallpaper from every source Ryoku uses, in priority order.
pub async fn detect() -> Option<Detected> {
    let mut sources: Vec<PathBuf> = Vec::new();
    if let Some(p) = from_swww().await { sources.push(p); }
    if let Some(p) = from_ryogami() { sources.push(p); }
    sources.extend(from_pointers());
    for path in sources {
        if let Some((mtime, size)) = fingerprint(&path) {
            return Some(Detected { path, mtime, size });
        }
    }
    None
}

pub fn is_video(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    VIDEO_EXT.contains(&ext.as_str())
}

/// Read the bytes the panel should display, extracting a 1920px video still when needed.
pub async fn serve_image(detected: &Detected) -> Option<ServedImage> {
    let src = &detected.path;
    if !is_video(src) {
        let bytes = tokio::fs::read(src).await.ok()?;
        let mime = mime_guess::from_path(src).first_or_octet_stream().to_string();
        return Some(ServedImage { bytes, mime });
    }

    let cache = home().join(".cache/ryokubox");
    let _ = tokio::fs::create_dir_all(&cache).await;
    let mut h = DefaultHasher::new();
    fingerprint(src).hash(&mut h);
    let out = cache.join(format!("frame-{:016x}.jpg", h.finish()));
    if !out.is_file() {
        let _ = tokio::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(src)
            .args(["-vf", "scale=1920:-1", "-frames:v", "1", "-q:v", "3"])
            .arg(&out)
            .output()
            .await;
    }
    let bytes = tokio::fs::read(&out).await.ok()?;
    Some(ServedImage { bytes, mime: "image/jpeg".into() })
}

/// A stable fingerprint of the palette file so SSE fires on colour changes too.
pub fn palette_fingerprint() -> u128 {
    let path = palette_path();
    match path.and_then(|p| fingerprint(&p)) {
        Some((m, s)) => {
            let mut h = DefaultHasher::new();
            m.hash(&mut h); s.hash(&mut h);
            h.finish() as u128
        }
        None => 0,
    }
}

fn palette_path() -> Option<PathBuf> {
    let candidates = [
        home().join(".cache/ryoku/colors.json"),
        home().join(".config/matugen/colors.json"),
        home().join(".cache/matugen/colors.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn color_hex(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => m.get("hex").and_then(|h| h.as_str()).map(|s| s.to_string()),
        _ => None,
    }
}

fn find_colors_root(v: &Value) -> Option<&Value> {
    for key in ["colors", "themePalette", "palette", "theme"] {
        if let Some(child) = v.get(key) {
            if child.get("primary").is_some() { return Some(child); }
        }
    }
    if v.get("primary").is_some() { Some(v) } else { None }
}

fn pick(colors: &Value, key: &str, fallback: &str) -> String {
    colors.get(key).and_then(color_hex).unwrap_or_else(|| fallback.to_string())
}

/// Read Matugen `colors.json` in Rust and mapMaterial You keys onto Lithic roles.
pub fn read_palette() -> Palette {
    let Some(path) = palette_path() else { return Palette::signature() };
    let Ok(raw) = std::fs::read_to_string(path) else { return Palette::signature() };
    let Ok(json) = serde_json::from_str::<Value>(&raw) else { return Palette::signature() };
    let Some(c) = find_colors_root(&json) else { return Palette::signature() };
    let sig = Palette::signature();

    let primary = pick(c, "primary", &sig.gold);
    let on_primary = pick(c, "onPrimary", &sig.on_gold);
    let primary_container = pick(c, "primaryContainer", &sig.gold_ink);
    let error = pick(c, "error", &sig.jasper);
    let on_error = pick(c, "onError", &sig.jasper_ink);
    let sec_container = pick(c, "secondaryContainer", &sig.sel);
    let on_sec_container = pick(c, "onSecondaryContainer", &sig.on_sel);
    let on_surface = pick(c, "onSurface", &sig.ink);
    let on_surface_variant = pick(c, "onSurfaceVariant", &sig.fg2);
    let surface = pick(c, "surface", &sig.ground);
    let surface_container = pick(c, "surfaceContainer", &sig.stone3);
    let surface_high = pick(c, "surfaceContainerHigh", &sig.stone4);
    let surface_highest = pick(c, "surfaceContainerHighest", &sig.field);
    let outline = pick(c, "outline", &sig.edge);
    let outline_variant = pick(c, "outlineVariant", &sig.vein);
    let scrim = pick(c, "scrim", &sig.scrim);

    Palette {
        gold: primary,
        on_gold: on_primary,
        gold_ink: primary_container,
        jasper: error,
        jasper_ink: on_error,
        sel: sec_container,
        on_sel: on_sec_container,
        ink: on_surface.clone(),
        fg: on_surface,
        fg2: on_surface_variant.clone(),
        fg3: on_surface_variant,
        ground: surface.clone(),
        stone1: surface.clone(),
        stone2: surface.clone(),
        stone3: surface_container,
        stone4: surface_high,
        slab: surface,
        field: surface_highest,
        vein: outline_variant,
        edge: outline,
        scrim,
        source: "matugen".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_paths_expand_to_home() {
        assert_eq!(expand_tilde("~/pics/a.png"), home().join("pics/a.png").to_string_lossy());
        assert_eq!(expand_tilde("/abs/a.png"), "/abs/a.png");
    }

    #[test]
    fn video_extensions_recognised() {
        assert!(is_video(Path::new("/x/a.mp4")));
        assert!(is_video(Path::new("/x/a.WEBM")));
        assert!(!is_video(Path::new("/x/a.png")));
    }

    #[test]
    fn maps_matugen_roles_onto_lithic_roles() {
        let raw = r##"{"colors":{
            "primary":{"hex":"#ff0000"},"onPrimary":{"hex":"#000000"},
            "primaryContainer":{"hex":"#ffaaaa"},"error":{"hex":"#00ff00"},
            "onError":{"hex":"#ffffff"},"secondaryContainer":{"hex":"#0000ff"},
            "onSecondaryContainer":{"hex":"#ffffff"},"onSurface":{"hex":"#eeeeee"},
            "onSurfaceVariant":{"hex":"#bbbbbb"},"surface":{"hex":"#101010"},
            "surfaceContainer":{"hex":"#202020"},"surfaceContainerHigh":{"hex":"#303030"},
            "surfaceContainerHighest":{"hex":"#404040"},"outline":{"hex":"#555555"},
            "outlineVariant":{"hex":"#444444"},"scrim":{"hex":"#000000"}
        }}"##;
        let json: Value = serde_json::from_str(raw).unwrap();
        let c = find_colors_root(&json).unwrap();
        assert_eq!(pick(c, "primary", ""), "#ff0000");
        assert_eq!(pick(c, "secondaryContainer", ""), "#0000ff");
        assert_eq!(pick(c, "error", ""), "#00ff00");
        assert_eq!(pick(c, "onSurface", ""), "#eeeeee");
    }
}
