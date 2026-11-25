/* Ryoku Box panel controller. Authenticated fetch SSE + Blob wallpaper preload. */

const $ = (s) => document.querySelector(s);
const $$ = (s) => [...document.querySelectorAll(s)];

const ROLE_MAP = {
  gold: '--lithic-gold', on_gold: '--lithic-on-gold', gold_ink: '--lithic-gold-ink',
  jasper: '--lithic-jasper', jasper_ink: '--lithic-jasper-ink',
  sel: '--lithic-sel', on_sel: '--lithic-on-sel',
  fg: '--lithic-fg', fg2: '--lithic-fg2', fg3: '--lithic-fg3',
  ground: '--lithic-ground', stone1: '--lithic-stone1', stone2: '--lithic-stone2',
  stone3: '--lithic-stone3', stone4: '--lithic-stone4', slab: '--lithic-slab',
  field: '--lithic-field', vein: '--lithic-vein', edge: '--lithic-edge',
  scrim: '--lithic-scrim',
};

/* Lithic signature.light from tokens.json — used when the user picks Light. */
const LIGHT_SIGNATURE = {
  gold: '#ce5e32', on_gold: '#000000', gold_ink: '#b04313',
  jasper: '#ba334a', jasper_ink: '#d9475c', sel: '#ce5e32', on_sel: '#000000',
  fg: '#1e1a15', fg2: '#56504a', fg3: '#6b655f',
  ground: '#e8e6e4', stone1: '#e8e6e4', stone2: '#e8e6e4', stone3: '#f0eeec',
  stone4: '#fbf9f7', slab: '#e8e6e4', field: '#f6f4f2', vein: '#cac7c3',
  edge: '#bcb9b5', scrim: '#00000073', source: 'signature-light',
};

let token = localStorage.getItem('ryokubox.token');
const isUsb = ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname);
let lastPalette = null;
let lastWpVersion = 0;
let wpFront = $('#wp-a');
let wpBack = $('#wp-b');
let wpObjectUrl = null;
let artObjectUrl = null;
let artUrlCurrent = '';
let customShortcuts = [];
let started = false;
let sliderBusy = false;
let lastAudio = null;

let prefMode = localStorage.getItem('rb.mode') || 'dark';
const prefContrast = localStorage.getItem('rb.contrast') || 'standard';
const prefReduce = localStorage.getItem('rb.reduce') === '1';

function showGate(msg) {
  $('#pair-gate').hidden = false;
  if (msg) $('#pair-msg').textContent = msg;
}

async function api(path, opts = {}) {
  const headers = Object.assign({ Authorization: `Bearer ${token || ''}` }, opts.headers || {});
  if (opts.body && !headers['Content-Type']) headers['Content-Type'] = 'application/json';
  const res = await fetch(path, { ...opts, headers });
  if (res.status === 401) showGate('Session expired. Pair again.');
  return res;
}

async function tryPair(code) {
  const res = await fetch('/api/pair', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code }),
  });
  if (!res.ok) return false;
  const j = await res.json();
  token = j.token;
  localStorage.setItem('ryokubox.token', token);
  if ($('#set-status')) $('#set-status').textContent = 'LINK ACTIVE';
  history.replaceState({}, '', location.pathname);
  $('#pair-gate').hidden = true;
  return true;
}

$('#pair-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const code = $('#pair-code').value.trim();
  if (!code) return;
  const ok = await tryPair(code);
  $('#pair-msg').textContent = ok ? '' : 'That code is invalid or has expired.';
  if (ok && !started) start();
});

/* ---------- Live roles from the matugen palette ---------- */
function writeRoles(palette) {
  const root = document.documentElement;
  root.setAttribute('data-l-snap', '');
  for (const [key, cssVar] of Object.entries(ROLE_MAP)) {
    if (palette[key]) root.style.setProperty(cssVar, palette[key]);
  }
  requestAnimationFrame(() => requestAnimationFrame(() => root.removeAttribute('data-l-snap')));
}

function applyRoles(palette) {
  if (!palette) return;
  lastPalette = palette;
  document.documentElement.setAttribute('data-l-mode', prefMode === 'light' ? 'light' : 'dark');
  writeRoles(prefMode === 'light' ? LIGHT_SIGNATURE : palette);
  updateHomeStrip();
}

/* ---------- Wallpaper: auth Blob, preload, crossfade ---------- */
async function setWallpaper(url, version) {
  try {
    const res = await api(url);
    if (!res.ok) return;
    const blob = await res.blob();
    const objUrl = URL.createObjectURL(blob);
    wpBack.style.backgroundImage = `url("${objUrl}")`;
    await new Promise((resolve) => {
      const img = new Image();
      img.onload = img.onerror = resolve;
      img.src = objUrl;
    });
    wpBack.classList.add('front');
    wpFront.classList.remove('front');
    const oldUrl = wpObjectUrl;
    [wpFront, wpBack] = [wpBack, wpFront];
    wpObjectUrl = objUrl;
    if (oldUrl && oldUrl !== objUrl) URL.revokeObjectURL(oldUrl);
    lastWpVersion = version;
    if (lastAudio) updateBackdrops();
  } catch (_) { /* keep last wallpaper on transient failure */ }
}

/* ---------- Authenticated SSE, parsed from a fetch stream ---------- */
async function startEvents() {
  try {
    const res = await api('/api/events');
    if (!res.ok || !res.body) return;
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let idx;
      while ((idx = buf.indexOf('\n\n')) >= 0) {
        const chunk = buf.slice(0, idx);
        buf = buf.slice(idx + 2);
        const line = chunk.split('\n').find((l) => l.startsWith('data:'));
        if (!line) continue;
        let ev;
        try { ev = JSON.parse(line.slice(5).trim()); } catch (_) { continue; }
        applyRoles(ev.palette);
        if (ev.version && ev.version !== lastWpVersion) {
          lastWpVersion = ev.version;
          setWallpaper(ev.wallpaper_url, ev.version);
        }
      }
    }
  } catch (_) { /* reconnect below */ }
  setTimeout(startEvents, 1500);
}

/* ---------- Clock and connection chip ---------- */
function tickClock() {
  const d = new Date();
  const s = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  $('#clock').textContent = s;
  $('#home-clock').textContent = s;
}
$('#conn-text').textContent = isUsb ? 'USB · LOCAL' : `WI-FI · ${location.hostname}`;

/* ---------- Sliders: 0..150, never fight the user's drag ---------- */
function paintSlider(field, slider, valueEl, value) {
  const min = Number(slider.min) || 0;
  const max = Number(slider.max) || 100;
  const fraction = Math.max(0, Math.min(1, (value - min) / (max - min)));
  field.style.setProperty('--l-value', fraction.toFixed(3));
  slider.value = String(Math.round(value));
  valueEl.textContent = `${Math.round(value)}%`;
}

function bindVolume(sliderId, fieldId, valueId) {
  const slider = $(sliderId);
  const send = () => api('/api/audio', {
    method: 'POST', body: JSON.stringify({ volume: Number(slider.value) }),
  }).catch(() => {});
  slider.addEventListener('pointerdown', () => { sliderBusy = true; });
  slider.addEventListener('pointerup', () => { sliderBusy = false; send(); });
  slider.addEventListener('input', (e) => {
    paintSlider($(fieldId), e.target, $(valueId), Number(e.target.value));
    clearTimeout(slider._timer);
    slider._timer = setTimeout(send, 120);
  });
  slider.addEventListener('change', send);
}
bindVolume('#volume-slider', '#volume-field', '#volume-value');
bindVolume('#media-volume-slider', '#media-volume-field', '#media-volume-value');

/* ---------- Levels disclosure ---------- */
$('#levels-toggle').addEventListener('click', () => {
  const btn = $('#levels-toggle');
  const disc = $('#levels-disclosure');
  const drawer = $('#levels-drawer');
  const open = btn.getAttribute('aria-expanded') === 'true';
  btn.setAttribute('aria-expanded', String(!open));
  if (open) disc.removeAttribute('data-l-open');
  else {
    drawer.style.setProperty('--l-disclosure-height', `${drawer.scrollHeight}px`);
    disc.setAttribute('data-l-open', '');
  }
});

/* ---------- Media transport ---------- */
function paintPlayButtons(playing) {
  $$('[data-media="play-pause"]').forEach((btn) => {
    const glyph = btn.querySelector('.l-glyph');
    if (glyph) glyph.textContent = playing ? 'pause' : 'play_arrow';
    btn.setAttribute('aria-label', playing ? 'Pause' : 'Play');
  });
}

$$('[data-media]').forEach((btn) => {
  btn.addEventListener('click', () => {
    if (btn.dataset.media === 'play-pause') {
      const glyph = btn.querySelector('.l-glyph');
      const wasPlaying = glyph && glyph.textContent === 'pause';
      paintPlayButtons(!wasPlaying); // optimistic; live poll corrects it
    }
    api(`/api/media/${btn.dataset.media}`, { method: 'POST' }).catch(() => {});
  });
});

function setBackdrop(el, url) {
  if (!el) return;
  el.style.backgroundImage = url ? `url("${url}")` : '';
}

/* Media art now lives only in the compact Media-page mini card. */
function updateBackdrops() {
  const art = artObjectUrl || wpObjectUrl;
  setBackdrop($('#mini-art'), art);
  setBackdrop($('#media-bg'), art);
}

/* Compact HOME strip: live link facts and the wallpaper's palette. */
let lastVitals = null;
function updateHomeStrip() {
  const net = $('#strip-network');
  const bt = $('#strip-bt');
  const up = $('#strip-uptime');
  const sw = $('#strip-swatches');
  if (!net || !bt || !up || !sw) return;
  net.textContent = connections.network || (connections.wifi ? 'WI-FI' : 'OFFLINE');
  bt.textContent = connections.bluetooth ? 'ON' : 'OFF';
  if (lastVitals && lastVitals.uptime != null) {
    const mins = Math.floor(lastVitals.uptime / 60);
    up.textContent = mins >= 60 ? `${Math.floor(mins / 60)}h ${mins % 60}m` : `${mins}m`;
  }
  sw.textContent = '';
  for (const role of ['gold', 'sel', 'jasper', 'stone2', 'field', 'fg']) {
    const color = lastPalette && lastPalette[role];
    if (!color) continue;
    const chip = document.createElement('i');
    chip.style.background = color;
    chip.title = `${role} · ${color}`;
    sw.append(chip);
  }
}

async function loadArt(url) {
  if (!url || url === artUrlCurrent) return;
  artUrlCurrent = url;
  let res;
  try { res = await api(`/api/art?url=${encodeURIComponent(url)}`); } catch (_) { return; }
  if (!res.ok) return;
  const blob = await res.blob();
  const obj = URL.createObjectURL(blob);
  const next = document.createElement('img');
  next.id = 'art-img';
  next.alt = 'Album art';
  next.src = obj;
  next.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;object-fit:cover;opacity:0;transition:opacity var(--rb-select) var(--rb-ease)';
  $('#art-frame').insertBefore(next, $('#art-empty'));
  await new Promise((resolve) => { next.onload = next.onerror = resolve; });
  $('#art-empty').hidden = true;
  $('#art-frame').style.backgroundImage = `url("${obj}")`;
  next.style.opacity = '1';
  const prev = artFront();
  const oldObj = artObjectUrl;
  artObjectUrl = obj;
  setTimeout(() => {
    if (prev && prev !== next) prev.remove();
    if (oldObj && oldObj !== obj) URL.revokeObjectURL(oldObj);
  }, 320);
  if (lastAudio) updateBackdrops();
}
function artFront() { return $('#art-frame img:last-child'); }

/* ---------- Tiles: connections + approved shortcuts ---------- */
const baseTiles = [
  { id: 'wifi', label: 'WI-FI', glyph: 'wifi', active: true },
  { id: 'bt', label: 'BLUETOOTH', glyph: 'bluetooth', active: false },
  { id: 'mute', label: 'MUTE', glyph: 'volume_off', active: false },
];
let connections = { wifi: true, bluetooth: false, network: null };

function renderTiles() {
  const grid = $('#tile-grid');
  grid.textContent = '';
  const all = [...baseTiles, ...customShortcuts.map((s) => ({
    id: `run:${s.id}`, label: s.label.toUpperCase(), glyph: 'bolt', active: false, shortcut: s.id,
  }))];
  for (const t of all) {
    const tile = document.createElement('div');
    tile.className = 'tile l-cut';
    tile.setAttribute('role', 'button');
    tile.tabIndex = 0;
    tile.setAttribute('aria-pressed', String(t.active));
    tile.dataset.tile = t.id;
    const row = document.createElement('span');
    const glyph = document.createElement('span');
    glyph.className = 'l-glyph'; glyph.setAttribute('aria-hidden', 'true'); glyph.textContent = t.glyph;
    row.append(glyph);
    const label = document.createElement('span');
    label.className = 't-label'; label.textContent = t.label;
    const value = document.createElement('span');
    value.className = 't-value'; value.textContent = t.active ? 'ON' : 'OFF';
    tile.append(row, label, value);
    const activate = () => {
      if (t.shortcut) api(`/api/run/${encodeURIComponent(t.shortcut)}`, { method: 'POST' }).catch(() => {});
      else if (t.id === 'mute') api('/api/audio', { method: 'POST', body: JSON.stringify({ toggle: true }) }).then(refreshAudio).catch(() => {});
      else if (t.id === 'wifi' || t.id === 'bt') {
        const target = t.id === 'bt' ? 'bluetooth' : 'wifi';
        const enabled = !(t.id === 'wifi' ? connections.wifi : connections.bluetooth);
        api(`/api/connections/${target}`, { method: 'POST', body: JSON.stringify({ enabled }) })
          .then(refreshConnections).catch(() => {});
      }
    };
    tile.addEventListener('click', activate);
    tile.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activate(); }
    });
    grid.append(tile);
  }
}

function setMuteTile(muted) {
  const tile = $('[data-tile="mute"]');
  if (!tile) return;
  tile.setAttribute('aria-pressed', String(muted));
  const value = tile.querySelector('.t-value');
  if (value) value.textContent = muted ? 'ON' : 'OFF';
}

async function loadShortcuts() {
  try {
    const j = await (await api('/api/shortcuts')).json();
    customShortcuts = j.shortcuts || [];
  } catch (_) { customShortcuts = []; }
  renderTiles();
  renderSettingsShortcuts();
}

/* ---------- Vitals and audio ---------- */
async function refreshVitals() {
  let v;
  try { v = await (await api('/api/vitals')).json(); } catch (_) { return; }
  lastVitals = v;
  $('#sum-cpu').textContent = `${Math.round(v.cpu)}%`;
  $('#sum-ram').textContent = `${Math.round(v.ram.percent)}%`;
  const rows = [
    ['CPU', `${Math.round(v.cpu)}%`, 'load'],
    ['RAM', `${Math.round(v.ram.percent)}%`, `${(v.ram.used / 1073741824).toFixed(1)} / ${(v.ram.total / 1073741824).toFixed(1)} GiB`],
  ];
  if (v.temp != null) rows.push(['TEMP', `${Math.round(v.temp)}°`, 'thermal']);
  if (v.battery != null) rows.push(['BATTERY', `${v.battery}%`, 'capacity']);
  const grid = $('#vitals-grid');
  grid.textContent = '';
  for (const [label, value, proof] of rows) {
    const card = document.createElement('div');
    card.className = 'vital l-cut';
    const l = document.createElement('span'); l.className = 'v-label'; l.textContent = label;
    const val = document.createElement('span'); val.className = 'v-value'; val.textContent = value;
    const p = document.createElement('span'); p.className = 'v-proof'; p.textContent = proof;
    card.append(l, val, p);
    grid.append(card);
  }
  updateHomeStrip();
}

function renderSinks(sinks, activeName) {
  const list = $('#sink-list');
  list.textContent = '';
  for (const s of sinks || []) {
    const chip = document.createElement('div');
    chip.className = 'sink l-cut';
    chip.setAttribute('aria-current', String(s.name === activeName));
    chip.textContent = s.description || s.name;
    list.append(chip);
  }
}

async function refreshAudio() {
  let a;
  try { a = await (await api('/api/audio')).json(); } catch (_) { return; }
  lastAudio = a;
  if (!sliderBusy) {
    paintSlider($('#volume-field'), $('#volume-slider'), $('#volume-value'), a.volume);
    paintSlider($('#media-volume-field'), $('#media-volume-slider'), $('#media-volume-value'), a.volume);
  }
  const player = a.player || 'no player';
  const sink = a.sink || 'no output';
  $('#player-name').textContent = player;
  $('#sink-name').textContent = sink;
  $('#sum-sink').textContent = sink.length > 18 ? `${sink.slice(0, 18)}…` : sink;
  $('#sum-sink-detail').textContent = 'audio sink';
  $('#track-title').textContent = a.title || 'Nothing playing';
  $('#track-artist').textContent = a.artist || '—';
  $('#track-source').textContent = `${player} · ${sink}`;
  $('#mini-title').textContent = a.title || '—';
  $('#mini-artist').textContent = a.artist || player;
  $('#home-foot').textContent = `LINK ACTIVE · ${sink} · ${isUsb ? 'USB' : 'WI-FI'}`;
  renderSinks(a.sinks, a.sink_name);
  setMuteTile(a.muted);
  paintPlayButtons(a.playing === true);
  document.body.classList.toggle('rb-playing', a.playing === true);
  updateBackdrops();
  if (a.art_url) loadArt(a.art_url);
}

function setTileState(id, active, text) {
  const tile = $(`[data-tile="${id}"]`);
  if (!tile) return;
  tile.setAttribute('aria-pressed', String(active));
  const value = tile.querySelector('.t-value');
  if (value) value.textContent = text;
}

async function refreshConnections() {
  try { connections = await (await api('/api/connections')).json(); } catch (_) { return; }
  setTileState('wifi', connections.wifi, connections.wifi ? (connections.network || 'ON') : 'OFF');
  setTileState('bt', connections.bluetooth, connections.bluetooth ? 'ON' : 'OFF');
  updateHomeStrip();
}

async function refreshAll() {
  await Promise.allSettled([refreshVitals(), refreshAudio(), refreshConnections()]);
}

/* ---------- Hold-to-activate session actions ---------- */
function setupHold(el) {
  const HOLD_MS = 900;
  let raf = null; let startTime = 0; let active = false; let done = false;
  const fill = el.querySelector('.hold-fill');
  const setFill = (p) => { fill.style.transform = `scaleX(${p})`; };
  const reset = () => {
    active = false; done = false; el.classList.remove('complete');
    if (raf) cancelAnimationFrame(raf);
    setFill(0);
  };
  const fire = () => {
    done = true; active = false; el.classList.add('complete');
    api(`/api/session/${el.dataset.session}`, { method: 'POST' }).catch(() => {});
    setTimeout(reset, 450);
  };
  const step = (t) => {
    if (!active) return;
    const p = Math.min(1, (t - startTime) / HOLD_MS);
    setFill(p);
    if (p >= 1) fire(); else raf = requestAnimationFrame(step);
  };
  const begin = () => {
    if (active || done) return;
    active = true; startTime = performance.now(); setFill(0);
    raf = requestAnimationFrame(step);
  };
  el.addEventListener('pointerdown', (e) => { e.preventDefault(); begin(); });
  el.addEventListener('pointerup', () => { if (active) reset(); });
  el.addEventListener('pointercancel', () => { if (active) reset(); });
  el.addEventListener('keydown', (e) => {
    if ((e.code === 'Space' || e.code === 'Enter') && !e.repeat) { e.preventDefault(); begin(); }
  });
  el.addEventListener('keyup', (e) => {
    if (e.code === 'Space' || e.code === 'Enter') { e.preventDefault(); if (active) reset(); }
  });
}
$$('.hold').forEach(setupHold);

/* ---------- Slides: big tabs + native swipe + arrow keys ---------- */
const pagesEl = $('#pages');
const tabs = $$('.page-tab');
function goToPage(index) {
  pagesEl.scrollTo({ left: index * pagesEl.clientWidth, behavior: 'smooth' });
}
tabs.forEach((tab) => tab.addEventListener('click', () => goToPage(Number(tab.dataset.page))));
/* One deliberate horizontal swipe = one slide, even when the browser's own
   momentum is aggressive. Control touches (sliders, buttons, tiles) are ignored. */
let touchStartX = 0, touchStartY = 0, touchOnControl = false;
pagesEl.addEventListener('touchstart', (e) => {
  if (e.touches.length !== 1) return;
  const t = e.touches[0];
  touchStartX = t.clientX; touchStartY = t.clientY;
  touchOnControl = !!(e.target.closest && e.target.closest('input,button,.tile,.hold,.seg,.shortcut-form'));
}, { passive: true });
pagesEl.addEventListener('touchend', (e) => {
  if (touchOnControl || !e.changedTouches.length) return;
  const t = e.changedTouches[0];
  const dx = t.clientX - touchStartX;
  const dy = t.clientY - touchStartY;
  if (Math.abs(dx) < 70 || Math.abs(dx) < Math.abs(dy) * 1.3) return;
  const current = Math.round(pagesEl.scrollLeft / pagesEl.clientWidth);
  const next = Math.max(0, Math.min(tabs.length - 1, current + (dx < 0 ? 1 : -1)));
  goToPage(next);
}, { passive: true });

const observer = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (entry.isIntersecting) {
      const index = [...entry.target.parentElement.children].indexOf(entry.target);
      tabs.forEach((t, i) => t.setAttribute('aria-current', String(i === index)));
      document.body.classList.toggle('on-home', index === 0);
    }
  }
}, { root: pagesEl, threshold: 0.6 });
$$('.page').forEach((p) => observer.observe(p));
document.addEventListener('keydown', (e) => {
  const current = tabs.findIndex((t) => t.getAttribute('aria-current') === 'true');
  if (e.key === 'ArrowRight' && current < tabs.length - 1) goToPage(current + 1);
  if (e.key === 'ArrowLeft' && current > 0) goToPage(current - 1);
});

/* ---------- Settings ---------- */
function initSettings() {
  $$('.settings-index-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      const i = btn.dataset.section;
      $$('.settings-index-item').forEach((x) => x.setAttribute('aria-current', String(x === btn)));
      $$('.settings-pane').forEach((p) => p.classList.toggle('active', p.dataset.section === i));
    });
  });

  document.documentElement.setAttribute('data-l-mode', prefMode === 'light' ? 'light' : 'dark');
  document.documentElement.setAttribute('data-l-contrast', prefContrast);
  if (prefReduce) document.documentElement.setAttribute('data-l-reduce', '');

  $$('#mode-seg button').forEach((b) => {
    b.classList.toggle('on', b.dataset.mode === prefMode);
    b.addEventListener('click', () => {
      prefMode = b.dataset.mode;
      localStorage.setItem('rb.mode', prefMode);
      $$('#mode-seg button').forEach((x) => x.classList.toggle('on', x === b));
      applyRoles(lastPalette || {});
    });
  });
  $$('#contrast-seg button').forEach((b) => {
    b.classList.toggle('on', b.dataset.contrast === prefContrast);
    b.addEventListener('click', () => {
      localStorage.setItem('rb.contrast', b.dataset.contrast);
      document.documentElement.setAttribute('data-l-contrast', b.dataset.contrast);
      $$('#contrast-seg button').forEach((x) => x.classList.toggle('on', x === b));
    });
  });
  const reduce = $('#reduce-toggle');
  reduce.setAttribute('aria-checked', String(prefReduce));
  reduce.textContent = prefReduce ? 'On' : 'Off';
  reduce.addEventListener('click', () => {
    const on = reduce.getAttribute('aria-checked') !== 'true';
    reduce.setAttribute('aria-checked', String(on));
    reduce.textContent = on ? 'On' : 'Off';
    localStorage.setItem('rb.reduce', on ? '1' : '0');
    if (on) document.documentElement.setAttribute('data-l-reduce', '');
    else document.documentElement.removeAttribute('data-l-reduce');
  });

  $('#link-host').value = location.host;
  $('#link-apply').addEventListener('click', () => {
    let host = $('#link-host').value.trim() || '127.0.0.1:8875';
    let base = host.includes('://') ? host : `http://${host}`;
    if (!base.endsWith('/')) base += '/';
    const code = $('#link-code').value.trim();
    location.href = base + (code ? `?pair=${encodeURIComponent(code)}` : '');
  });

  $('#set-host').textContent = location.host;
  $('#set-host-about').textContent = location.host;
  $('#set-status').textContent = token ? 'LINK ACTIVE' : 'PAIRING REQUIRED';
  $('#set-transport').textContent = isUsb ? 'USB (adb reverse)' : 'Wi-Fi · auto-linked';
  $('#btn-reload').addEventListener('click', () => location.reload());
  const showToken = () => { $('#set-token').textContent = token ? `${token.slice(0, 8)}…${token.slice(-4)}` : '—'; };
  showToken();
  $('#btn-refresh-wp').addEventListener('click', () => setWallpaper('/api/wallpaper', lastWpVersion + 1));
  $('#btn-rotate').addEventListener('click', async () => {
    const res = await api('/api/token/rotate', { method: 'POST' });
    if (!res.ok) { $('#set-token').textContent = 'loopback only'; return; }
    const j = await res.json();
    token = j.token;
    localStorage.setItem('ryokubox.token', token);
    showToken();
  });

  $('#shortcut-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const label = $('#sc-label').value.trim();
    const cmd = $('#sc-cmd').value.trim();
    if (!label || !cmd) return;
    const argv = cmd.split(/\s+/);
    const next = [...customShortcuts, {
      id: `sc-${Date.now().toString(36)}`, label, argv, approved: true,
    }];
    await api('/api/shortcuts', { method: 'PUT', body: JSON.stringify(next) }).catch(() => {});
    $('#sc-label').value = ''; $('#sc-cmd').value = '';
    await loadShortcuts();
  });
}
initSettings();

function renderSettingsShortcuts() {
  const host = $('#settings-shortcuts');
  if (!host) return;
  host.textContent = '';
  const count = $('#sc-count');
  if (count) count.textContent = `${customShortcuts.length} shortcut${customShortcuts.length === 1 ? '' : 's'}`;
  for (const sc of customShortcuts) {
    const row = document.createElement('div');
    row.className = 'shortcut-row';
    const name = document.createElement('span'); name.className = 'sc-name'; name.textContent = sc.label;
    const cmd = document.createElement('span'); cmd.className = 'sc-cmd'; cmd.textContent = sc.argv.join(' ');
    const del = document.createElement('button');
    del.textContent = '×'; del.setAttribute('aria-label', `Remove ${sc.label}`);
    del.addEventListener('click', async () => {
      const next = customShortcuts.filter((x) => x.id !== sc.id);
      await api('/api/shortcuts', { method: 'PUT', body: JSON.stringify(next) }).catch(() => {});
      await loadShortcuts();
    });
    row.append(name, cmd, del);
    host.append(row);
  }
}

/* ---------- Boot ---------- */
function start() {
  if (started) return;
  started = true;
  tickClock();
  setInterval(tickClock, 1000);
  setInterval(refreshAll, 2000);
  refreshAll();
  loadShortcuts();
  startEvents();
}

/* Native Android volume keys bridge: adjust the PC sink, not the phone. */
window.rbAdjustVolume = async (delta) => {
  try {
    const cur = await (await api('/api/audio')).json();
    const next = Math.max(0, Math.min(100, cur.volume + delta));
    paintSlider($('#volume-field'), $('#volume-slider'), $('#volume-value'), next);
    paintSlider($('#media-volume-field'), $('#media-volume-slider'), $('#media-volume-value'), next);
    const res = await api('/api/audio', { method: 'POST', body: JSON.stringify({ volume: next }) });
    const j = await res.json().catch(() => ({}));
    if (j.error) { $('#home-foot').textContent = `VOLUME: ${j.error}`; }
    setTimeout(refreshAudio, 300);
  } catch (_) { /* keep current volume */ }
};

/* Deep link a slide with #0..#3 (handy for kiosk mode and debugging). */
const hashPage = parseInt(location.hash.slice(1), 10);
function jumpToHash() {
  if (Number.isNaN(hashPage)) return;
  const i = Math.max(0, Math.min(3, hashPage));
  pagesEl.scrollTo({ left: i * pagesEl.clientWidth, behavior: 'instant' });
  tabs.forEach((t, k) => t.setAttribute('aria-current', String(k === i)));
}
jumpToHash();
window.addEventListener('load', jumpToHash);

(async () => {
  if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
  const code = new URLSearchParams(location.search).get('pair');
  if (code) {
    const ok = await tryPair(code);
    if (ok) start(); else showGate('That pairing code is invalid or has expired.');
  } else if (token) {
    try {
      const res = await api('/api/ping');
      if (res.ok) start(); else showGate('Pair with this device to continue.');
    } catch (_) { showGate('The agent is not answering yet. Retrying.'); setTimeout(() => location.reload(), 2000); }
  } else {
    showGate();
  }
})();
