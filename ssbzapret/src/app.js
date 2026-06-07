/* =====================================================================
   SSBZapret — фронтенд.
   Один и тот же UI работает в двух режимах:
     • в Tauri-приложении — через invoke()/listen() к Rust-бэкенду;
     • в обычном браузере — через MockBackend (для превью и разработки).
   ===================================================================== */

/* ---------- встроенные пресеты (зеркало store.rs) ---------- */
const BUILTINS = [
  { id: 'ai_bypass', name: 'AI Bypass', icon: 'ai', builtin: true,
    services: ['ChatGPT', 'Claude', 'Gemini', 'Grok'],
    strategy: { method: 'fakeddisorder', ttl: 8, split_pos: 'sni' },
    domains: ['openai.com','chatgpt.com','oaistatic.com','claude.ai','anthropic.com','gemini.google.com','bard.google.com','x.ai','grok.com'] },
  { id: 'social', name: 'Social', icon: 'social', builtin: true,
    services: ['WhatsApp', 'Telegram', 'YouTube'],
    strategy: { method: 'fake', ttl: 6, split_pos: 'sni' },
    domains: ['whatsapp.com','whatsapp.net','telegram.org','t.me','telegram-cdn.org','youtube.com','googlevideo.com','ytimg.com'] },
  { id: 'games', name: 'Games', icon: 'games', builtin: true,
    services: ['Roblox'],
    strategy: { method: 'split', ttl: 8, split_pos: 'sni' },
    domains: ['roblox.com','rbxcdn.com'] },
];

const METHODS = [
  { id: 'split',         name: 'Split',          desc: 'Разрез ClientHello по SNI' },
  { id: 'fake',          name: 'Fake TLS',       desc: 'Ложный пакет + низкий TTL' },
  { id: 'disorder',      name: 'Disorder',       desc: 'Обратный порядок сегментов' },
  { id: 'fakeddisorder', name: 'Fake+Disorder',  desc: 'Самый устойчивый режим' },
];

// Зеркало списка DNS-провайдеров из store.rs (используется в превью/браузере).
const DNS_PROVIDERS = [
  { id:'comss', name:'Comss.one DNS', note:'Разблокирует ИИ: ChatGPT, Gemini, Copilot, Claude. Рекомендуется.', ips:['83.220.169.155','212.109.195.93'], doh:'https://dns.comss.one/dns-query' },
  { id:'xbox', name:'Xbox DNS', note:'ИИ + игры (Xbox Live, Supercell). Альтернатива Comss.', ips:['176.99.11.77'], doh:'https://xbox-dns.ru/dns-query' },
  { id:'cloudflare', name:'Cloudflare', note:'Шифрованный DNS от подмены провайдером. ИИ сам по себе не открывает.', ips:['1.1.1.1','1.0.0.1'], doh:'https://cloudflare-dns.com/dns-query' },
  { id:'google', name:'Google Public DNS', note:'Публичный DNS Google. Базовая защита от подмены DNS.', ips:['8.8.8.8','8.8.4.4'], doh:'https://dns.google/dns-query' },
];

const clone = (o) => JSON.parse(JSON.stringify(o));

/* ---------- монохромные SVG-иконки (заменяют эмодзи) ---------- */
const SVG = {
  ai: '<rect x="4" y="8" width="16" height="12" rx="2"/><path d="M12 8V5"/><circle cx="12" cy="3.5" r="1.2"/><path d="M9 13h.01M15 13h.01M9 16.5h6"/><path d="M2 13v2M22 13v2"/>',
  social: '<path d="M21 11.5a8.38 8.38 0 0 1-9 8.5 9 9 0 0 1-4-1L3 20l1-4a8.5 8.5 0 0 1 5-12 8.38 8.38 0 0 1 12 7.5z"/>',
  games: '<rect x="2" y="6" width="20" height="12" rx="6"/><path d="M6 12h4M8 10v4M15 13h.01M18 11h.01"/>',
  shield: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
  scissors: '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M20 4 8.12 15.88M14.47 14.48 20 20M8.12 8.12 12 12"/>',
  mask: '<path d="M3 5s2 3 9 3 9-3 9-3v6a9 9 0 0 1-18 0z"/><path d="M8.5 12h.01M15.5 12h.01"/>',
  shuffle: '<path d="M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5"/>',
  bolt: '<path d="M13 2 3 14h8l-1 8 11-13h-8z"/>',
  antenna: '<circle cx="12" cy="12" r="2"/><path d="M16.2 7.8a6 6 0 0 1 0 8.5M7.8 16.2a6 6 0 0 1 0-8.5M19 4.9a10 10 0 0 1 0 14.2M5 19.1A10 10 0 0 1 5 4.9"/>',
  globe: '<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20 15 15 0 0 1 0-20z"/>',
  rocket: '<path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="M12 15l-3-3a22 22 0 0 1 2-4A12.9 12.9 0 0 1 22 2c0 2.72-.78 7.5-6 11a22 22 0 0 1-4 2z"/><path d="M9 12H4s.55-3 2-4c1.62-1.08 5 0 5 0M12 15v5s3-.55 4-2c1.08-1.62 0-5 0-5"/>',
  lock: '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  star: '<path d="m12 2 3.1 6.3 6.9 1-5 4.9 1.2 6.9L12 17.8l-6.2 3.3L7 14.2l-5-4.9 6.9-1z"/>',
  wifi: '<path d="M5 12.5a11 11 0 0 1 14 0M8.5 16a6 6 0 0 1 7 0M2 8.8a15 15 0 0 1 20 0M12 20h.01"/>',
  cloud: '<path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/>',
  key: '<circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6M15.5 7.5l3 3L22 7l-3-3"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
};
/* набор иконок для выбора в редакторе пресета */
const ICON_CHOICES = ['ai','social','games','shield','bolt','globe','rocket','lock','star','wifi','cloud','key','scissors','mask','shuffle','antenna'];
function svgIcon(key) {
  const body = SVG[key] || SVG.globe;
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;
}
/* миграция старых эмодзи-значений в ключи иконок */
const EMOJI_TO_KEY = { '\u{1F916}':'ai', '\u{1F4AC}':'social', '\u{1F3AE}':'games', '\u{1F310}':'globe', '\u{1F6E1}':'shield', '\u26A1':'bolt', '\u{1F680}':'rocket', '\u{1F512}':'lock', '\u2B50':'star' };
function iconKey(v) {
  if (!v) return 'globe';
  if (SVG[v]) return v;
  const bare = v.replace(/\uFE0F/g, '');
  return EMOJI_TO_KEY[v] || EMOJI_TO_KEY[bare] || 'globe';
}
function presetIcon(p) { return svgIcon(iconKey(p && p.icon)); }

/* суммарное число доменов пресета: свои + встроенные правила */
function presetDomainCount(p) {
  if (!p) return 0;
  let n = (p.domains || []).length;
  if (Array.isArray(p.rules)) for (const r of p.rules) n += (r.domains || []).length;
  return n;
}

/* ===================================================================
   BACKEND — единый интерфейс
   =================================================================== */
const TAURI = !!(window.__TAURI__ && window.__TAURI__.core);

class TauriBackend {
  constructor() {
    this.invoke = window.__TAURI__.core.invoke;
    this.listen = window.__TAURI__.event.listen;
  }
  getStatus()              { return this.invoke('get_status'); }
  start(id)                { return this.invoke('start_engine', { preset_id: id }); }
  stop()                   { return this.invoke('stop_engine'); }
  listPresets()            { return this.invoke('list_presets'); }
  getActivePreset()        { return this.invoke('get_active_preset'); }
  setActivePreset(id)      { return this.invoke('set_active_preset', { preset_id: id }); }
  savePreset(p)            { return this.invoke('save_preset', { preset: p }); }
  deletePreset(id)         { return this.invoke('delete_preset', { preset_id: id }); }
  addDomain(id, d)         { return this.invoke('add_domain', { preset_id: id, domain: d }); }
  removeDomain(id, d)      { return this.invoke('remove_domain', { preset_id: id, domain: d }); }
  setStrategy(id, s)       { return this.invoke('set_strategy', { preset_id: id, strategy: s }); }
  probeDomains(list)       { return this.invoke('probe_domains', { domains: list }); }
  autotune(id)             { return this.invoke('autotune', { preset_id: id }); }
  engineReady()            { return this.invoke('engine_ready'); }
  listDnsProviders()       { return this.invoke('list_dns_providers'); }
  getDns()                 { return this.invoke('get_dns'); }
  setDns(id)               { return this.invoke('set_dns', { provider_id: id }); }
  checkUpdate()            { return this.invoke('check_update'); }
  installUpdate()          { return this.invoke('install_update'); }
  onStats(cb)  { this.listen('engine://stats',  (e) => cb(e.payload)); }
  onLog(cb)    { this.listen('engine://log',    (e) => cb(e.payload)); }
  onStatus(cb) { this.listen('engine://status', (e) => cb(e.payload)); }
}

class MockBackend {
  constructor() {
    this._load();
    this.running = false;
    this.started = 0;
    this.subs = { stats: [], log: [], status: [] };
    this._statTimer = null;
    this._logTimer = null;
    this._processed = 0;
  }
  _load() {
    try {
      const raw = JSON.parse(localStorage.getItem('ssb_store') || 'null');
      this.presets = raw?.presets || clone(BUILTINS);
      this.activeId = raw?.activeId || 'ai_bypass';
      this.dnsProvider = raw?.dnsProvider || 'off';
    } catch { this.presets = clone(BUILTINS); this.activeId = 'ai_bypass'; this.dnsProvider = 'off'; }
    // гарантируем встроенные
    for (const b of BUILTINS) if (!this.presets.some(p => p.id === b.id)) this.presets.push(clone(b));
  }
  _save() { localStorage.setItem('ssb_store', JSON.stringify({ presets: this.presets, activeId: this.activeId, dnsProvider: this.dnsProvider })); }
  _find(id) { return this.presets.find(p => p.id === id); }
  _emit(ch, v) { this.subs[ch].forEach(cb => cb(v)); }
  _status() { return { running: this.running, uptime_secs: this.running ? ((Date.now()-this.started)/1000|0) : 0, preset_id: this.activeId }; }

  async getStatus() { return this._status(); }
  async start(id) {
    this.activeId = id; this._save();
    this.running = true; this.started = Date.now();
    const p = this._find(id);
    this._emit('status', this._status());
    this._emit('log', { level:'ok', tag:'engine', msg:`движок запущен · пресет ${p?.name} · метод ${p?.strategy.method}` });
    this._startTimers();
  }
  async stop() {
    this.running = false;
    this._emit('status', this._status());
    this._emit('log', { level:'warn', tag:'engine', msg:'движок остановлен' });
    this._stopTimers();
  }
  _startTimers() {
    this._stopTimers();
    let t = 0;
    this._statTimer = setInterval(() => {
      if (!this.running) return;
      t++;
      const p = this._find(this.activeId);
      const pps = 900 + (t*37)%600;
      this._processed += pps;
      this._emit('stats', { pkt_s: pps, processed: this._processed, active_domains: p?.domains.length||0, mbit: 70 + (t%25) });
    }, 1000);
    const samples = [['ok','tls','split ClientHello → %d · OK'],['inf','probe','fake+split2 confirmed · %d'],['ok','tls','desync applied · %d'],['warn','quic','QUIC dropped → fallback TCP'],['ok','nfq','reorder segments · %d']];
    this._logTimer = setInterval(() => {
      if (!this.running) return;
      const p = this._find(this.activeId);
      const s = samples[Math.random()*samples.length|0];
      const d = p?.domains?.[Math.random()*(p.domains.length||1)|0] || 'host';
      this._emit('log', { level:s[0], tag:s[1], msg:s[2].replace('%d', d) });
    }, 2200);
  }
  _stopTimers() { clearInterval(this._statTimer); clearInterval(this._logTimer); this._statTimer=this._logTimer=null; }

  async listPresets() { return clone(this.presets); }
  async getActivePreset() { return this.activeId; }
  async setActivePreset(id) { this.activeId = id; this._save(); this._emit('status', this._status()); }
  async savePreset(p) {
    const ex = this._find(p.id);
    if (ex) {
      if (ex.builtin) { ex.domains = p.domains; ex.strategy = p.strategy; ex.services = p.services; }
      else Object.assign(ex, p);
    } else this.presets.push(clone(p));
    this._save();
  }
  async deletePreset(id) {
    const p = this._find(id);
    if (p?.builtin) throw new Error('встроенный пресет нельзя удалить');
    this.presets = this.presets.filter(x => x.id !== id);
    if (this.activeId === id) this.activeId = 'ai_bypass';
    this._save();
  }
  async addDomain(id, d) {
    const p = this._find(id); d = d.trim().toLowerCase();
    if (d && !p.domains.includes(d)) p.domains.push(d);
    this._save(); return clone(p.domains);
  }
  async removeDomain(id, d) {
    const p = this._find(id); p.domains = p.domains.filter(x => x !== d);
    this._save(); return clone(p.domains);
  }
  async setStrategy(id, s) { const p = this._find(id); p.strategy = s; this._save(); }
  async probeDomains(list) {
    await new Promise(r => setTimeout(r, 600));
    return list.map(d => {
      const blocked = Math.random() < 0.6;
      const split = blocked && Math.random() < 0.5;
      return { domain: d, direct_ok: !blocked, split_ok: !blocked || split, ms: 40 + (Math.random()*420|0),
        detail: blocked ? (split ? 'обходится через split' : 'заблокирован') : 'доступен напрямую' };
    });
  }
  async autotune(id) {
    const p = this._find(id);
    const sample = (p ? p.domains : []).slice(0, 4);
    const cands = [
      ['fakeddisorder_md5','Fake+Disorder · md5sig · TTL2'],
      ['fakeddisorder_badseq','Fake+Disorder · badseq · TTL4'],
      ['fake_ttl3','Fake · TTL3 · md5sig'],
      ['split2_midsld','Split2 · midsld'],
      ['disorder2','Disorder2 · midsld'],
      ['fake_quic','Fake · TTL3 + QUIC off'],
    ];
    await new Promise(r => setTimeout(r, 500));
    let best = -1, bi = 0;
    const rows = cands.map((c, i) => {
      const ok = Math.round(Math.random() * sample.length);
      if (ok > best) { best = ok; bi = i; }
      return { id: c[0], name: c[1], ok, total: sample.length, applied: false };
    });
    if (rows[bi]) rows[bi].applied = true;
    return rows;
  }
  async engineReady() { return false; }
  async listDnsProviders() { return clone(DNS_PROVIDERS); }
  async getDns() { return this.dnsProvider || 'off'; }
  async setDns(id) { this.dnsProvider = id; this._save(); this._emit('log', { level:'ok', tag:'dns', msg:'DNS (превью): ' + id }); }
  onStats(cb)  { this.subs.stats.push(cb); }
  onLog(cb)    { this.subs.log.push(cb); }
  onStatus(cb) { this.subs.status.push(cb); }
}

const B = TAURI ? new TauriBackend() : new MockBackend();

/* ===================================================================
   СОСТОЯНИЕ UI
   =================================================================== */
const UI = { presets: [], activeId: 'ai_bypass', running: false, uptime: 0 };
const $ = (id) => document.getElementById(id);
const el = (tag, cls, html) => { const e=document.createElement(tag); if(cls)e.className=cls; if(html!=null)e.innerHTML=html; return e; };
const esc = (s) => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const active = () => UI.presets.find(p => p.id === UI.activeId);

/* ===================================================================
   НАВИГАЦИЯ
   =================================================================== */
const navItems = [...document.querySelectorAll('.nav-item')];
const views = {};
document.querySelectorAll('.view').forEach(v => views[v.id] = v);
function go(id) {
  navItems.forEach(i => i.classList.toggle('active', i.dataset.view === id));
  Object.entries(views).forEach(([k, v]) => v.classList.toggle('active', k === id));
}
navItems.forEach(i => i.addEventListener('click', () => go(i.dataset.view)));
document.querySelectorAll('[data-view]:not(.nav-item)').forEach(b => b.addEventListener('click', () => go(b.dataset.view)));

/* generic toggles */
document.addEventListener('click', e => { const t = e.target.closest('[data-toggle]'); if (t) t.classList.toggle('on'); });

/* ===================================================================
   ТЕМЫ ОФОРМЛЕНИЯ
   =================================================================== */
const THEMES = [
  { id:'mono',       name:'Stronger (моно)', sw:['#0b0b0d','#f1f4f8','#9aa1ab'] },
  { id:'white',      name:'Светлая',         sw:['#ffffff','#23262f','#5b6170'] },
  { id:'homelander', name:'Stronger, Smarter, Better', sw:['#0a0e1a','#e63946','#e4b53b'] },
  { id:'gamesense',  name:'Gamesense',        sw:['#141414','#a6cf5a','#7fa341'] },
  { id:'ocean',      name:'Ocean',            sw:['#06121a','#22d3ee','#3b82f6'] },
  { id:'amethyst',   name:'Amethyst',         sw:['#100a1a','#a855f7','#d946ef'] },
  { id:'ember',      name:'Ember',            sw:['#160c08','#fb923c','#f43f5e'] },
];
function renderThemes() {
  const box = $('theme-grid');
  if (!box) return;
  const cur = document.documentElement.getAttribute('data-theme') || 'mono';
  box.innerHTML = THEMES.map(t => `<button class="theme-sw${t.id===cur?' on':''}" data-theme-id="${t.id}">`+
    `<span class="theme-sw-pv" style="background:${t.sw[0]}"><i style="background:${t.sw[1]};color:${t.sw[1]}"></i><i style="background:${t.sw[2]};color:${t.sw[2]}"></i></span>`+
    `<span class="theme-sw-nm">${esc(t.name)}</span></button>`).join('');
  box.querySelectorAll('[data-theme-id]').forEach(b => b.addEventListener('click', () => applyTheme(b.dataset.themeId)));
}
function applyTheme(id) {
  document.documentElement.setAttribute('data-theme', id);
  try { localStorage.setItem('ssb_theme', id); } catch (e) {}
  renderThemes();
  const t = THEMES.find(x => x.id === id);
  pushLog({ level:'inf', tag:'ui', msg: 'тема: ' + (t ? t.name : id) });
}

async function renderDns() {
  const box = $('dns-grid');
  if (!box) return;
  let provs = [];
  try { provs = await B.listDnsProviders(); } catch (e) { provs = []; }
  let cur = 'off';
  try { cur = await B.getDns(); } catch (e) {}
  const off = { id:'off', name:'Выключено', note:'Системный DNS по умолчанию (от провайдера / DHCP).', ips:[], doh:'' };
  const all = [off].concat(provs || []);
  box.innerHTML = all.map(p => `\n    <button class="dns-sw${p.id === cur ? ' on' : ''}" data-dns-id="${p.id}">\n      <span class="dns-sw-nm">${esc(p.name)}</span>\n      <span class="dns-sw-note">${esc(p.note)}</span>\n      ${p.ips && p.ips.length ? `<span class="dns-sw-ip">${esc(p.ips.join(' · '))}</span>` : ''}\n    </button>`).join('');
  box.querySelectorAll('[data-dns-id]').forEach(b => b.addEventListener('click', () => applyDns(b.dataset.dnsId)));
}

async function applyDns(id) {
  const box = $('dns-grid');
  if (box) box.querySelectorAll('[data-dns-id]').forEach(b => { b.disabled = true; });
  pushLog({ level:'inf', tag:'dns', msg: id === 'off' ? 'сброс системного DNS…' : 'переключение DNS…' });
  try {
    await B.setDns(id);
    pushLog({ level:'ok', tag:'dns', msg: id === 'off' ? 'DNS сброшен на системный по умолчанию' : ('DNS применён: ' + id) });
  } catch (e) {
    pushLog({ level:'err', tag:'dns', msg: 'не удалось применить DNS: ' + ((e && e.message) || e) });
  }
  await renderDns();
}


/* ===================================================================
   ЛОГИ
   =================================================================== */
const miniLog = $('mini-log'), fullLog = $('full-log');
const LV = { ok:'OK', warn:'WARN', err:'ERR', inf:'INFO' };
const nowStr = () => new Date().toLocaleTimeString('ru-RU', { hour12:false });
function line(e) {
  const d = el('div', 'ln');
  d.innerHTML = `<span class="t">${nowStr()}</span><span class="lv ${e.level}">${LV[e.level]||'INFO'}</span><span class="msg">[${esc(e.tag)}] ${esc(e.msg)}</span>`;
  return d;
}
function pushLog(e) {
  [miniLog, fullLog].forEach(c => { if(!c) return; const l = line(e); c.appendChild(l); c.scrollTop = c.scrollHeight; while (c.childElementCount > 200) c.removeChild(c.firstChild); });
}
if ($('clear-log')) $('clear-log').addEventListener('click', () => { fullLog.innerHTML = ''; });

/* ===================================================================
   СТАТУС / ПИТАНИЕ
   =================================================================== */
const hero = $('hero'), badge = $('badge'), hdrDot = $('hdr-dot'), hdrState = $('hdr-state');
const heroTitle = $('hero-title'), heroDesc = $('hero-desc');
function renderStatus(st) {
  UI.running = st.running;
  if (st.running && st.preset_id) UI.activeId = st.preset_id;
  if (hero)    hero.classList.toggle('off', !st.running);
  if (hdrDot)  hdrDot.classList.toggle('off', !st.running);
  if (hdrState) hdrState.textContent = st.running ? 'ЗАЩИЩЕНО' : 'ОТКЛЮЧЕНО';
  if (badge)   badge.innerHTML = st.running ? '<span class="dot"></span> АКТИВНО' : '<span class="dot off"></span> ОСТАНОВЛЕНО';
  if (heroTitle) heroTitle.textContent = st.running ? 'Под защитой' : 'Защита выключена';
  if (heroDesc) heroDesc.textContent = st.running
    ? 'Трафик к выбранным доменам проходит через движок обхода DPI. Остальной трафик идёт напрямую и не теряет в скорости.'
    : 'Движок остановлен. Весь трафик идёт напрямую и может фильтроваться провайдером.';
  syncHero();
}
if ($('power')) $('power').addEventListener('click', async () => {
  if (UI.running) await B.stop();
  else await B.start(UI.activeId);
});

/* отражаем активный пресет на дашборде */
function syncHero() {
  const p = active();
  if (!p) return;
  if ($('hero-strat')) $('hero-strat').textContent = p.name;
  if ($('hero-profile')) $('hero-profile').textContent = p.name;
  if ($('st-dom') && !UI.running) $('st-dom').textContent = presetDomainCount(p);
}

/* ===================================================================
   ДАШБОРД: uptime / sparks / pps
   =================================================================== */
let localUptime = 0;
setInterval(() => {
  if (UI.running) localUptime++;
  if ($('uptime')) {
    const h = String(localUptime/3600|0).padStart(2,'0'), m = String(localUptime%3600/60|0).padStart(2,'0'), s = String(localUptime%60).padStart(2,'0');
    $('uptime').textContent = `${h}:${m}:${s}`;
  }
}, 1000);

function spark(id) { const c = $(id); if(!c) return; for (let i=0;i<22;i++){ const b=el('i'); b.style.height=(20+Math.random()*80)+'%'; c.appendChild(b);} }
['sp1','sp2','sp3'].forEach(spark);
setInterval(() => {
  if (!UI.running) return;
  ['sp1','sp2','sp3'].forEach(id => { const c=$(id); if(!c)return; for (const b of c.children) b.style.height=(20+Math.random()*80)+'%'; });
}, 2000);

function renderStats(s) {
  const live = UI.running;
  // В реальном режиме winws не отдаёт per-packet статистику (учёт идёт
  // внутри процесса), поэтому при нулях показываем «активно», а не
  // зависшие нули.
  if ($('pps')) $('pps').textContent = s.pkt_s ? (s.pkt_s.toLocaleString('ru-RU') + ' pkt/s') : (live ? 'активно' : '0 pkt/s');
  if ($('st-pkt')) $('st-pkt').textContent = s.processed ? s.processed.toLocaleString('ru-RU') : (live ? '—' : '0');
  if ($('st-spd')) $('st-spd').innerHTML = s.mbit ? (s.mbit + '<u>Mbit/s</u>') : (live ? '<u>активно</u>' : '0<u>Mbit/s</u>');
  if ($('st-dom')) $('st-dom').textContent = s.active_domains ?? presetDomainCount(active());
}

/* ===================================================================
   ПРЕСЕТЫ (вкладка «Стратегии»)
   =================================================================== */
const stratGrid = $('strat-grid');
function renderPresets() {
  if (!stratGrid) return;
  stratGrid.innerHTML = '';
  for (const p of UI.presets) {
    const card = el('div', 'strat-card' + (p.id === UI.activeId ? ' sel' : ''));
    card.dataset.id = p.id;
    const pills = (p.services||[]).map(s => `<span class="svc-pill">${esc(s)}</span>`).join('');
    card.innerHTML = `
      <div class="sc-top">
        <div class="sc-ic">${presetIcon(p)}</div>
        <div class="sc-meta">
          <h3>${esc(p.name)}</h3>
          <span class="sc-sub">${p.builtin ? 'встроенный' : 'пользовательский'} · ${presetDomainCount(p)} дом.</span>
        </div>
        <div class="sc-check">${svgIcon('check')}</div>
      </div>
      <div class="sc-svcs">${pills || '<span class="svc-pill dim">без ярлыков</span>'}</div>
      <div class="sc-foot">
        <span class="sc-method">${esc(p.strategy.method)}</span>
        <div class="sc-actions">
          <button class="sc-btn" data-act="edit">Настроить</button>
          ${p.builtin ? '' : '<button class="sc-btn danger" data-act="del">Удалить</button>'}
        </div>
      </div>`;
    card.addEventListener('click', async (e) => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'edit') { e.stopPropagation(); openEditor(p.id); return; }
      if (act === 'del')  { e.stopPropagation(); if (confirm(`Удалить пресет «${p.name}»?`)) { await B.deletePreset(p.id); await reloadPresets(); } return; }
      await selectPreset(p.id);
    });
    stratGrid.appendChild(card);
  }
  // карточка «создать»
  const add = el('div', 'strat-card add');
  add.innerHTML = '<div class="add-plus">+</div><div class="add-txt">Создать пресет</div>';
  add.addEventListener('click', () => openEditor(null));
  stratGrid.appendChild(add);
}

async function selectPreset(id) {
  UI.activeId = id;
  await B.setActivePreset(id);
  if (UI.running) await B.start(id); // горячее применение
  renderPresets(); renderDomains(); syncHero();
  pushLog({ level:'inf', tag:'preset', msg:'выбран пресет: ' + (active()?.name||id) });
}

/* ===================================================================
   РЕДАКТОР ПРЕСЕТА (модалка)
   =================================================================== */
let modal;
function setMethodValue(v) {
  const m = METHODS.find(x => x.id === v) || METHODS[0];
  modal.querySelector('#m-method').value = m.id;
  modal.querySelector('#m-method-btn .dd-val').textContent = `${m.name} — ${m.desc}`;
  modal.querySelectorAll('#m-method-menu .dd-opt').forEach(o => o.classList.toggle('sel', o.dataset.id === m.id));
}
function buildMethodDD() {
  const dd = modal.querySelector('#m-method-dd');
  const btn = modal.querySelector('#m-method-btn');
  const menu = modal.querySelector('#m-method-menu');
  menu.innerHTML = METHODS.map(m => `<button type="button" class="dd-opt" data-id="${m.id}"><b>${esc(m.name)}</b><small>${esc(m.desc)}</small></button>`).join('');
  btn.addEventListener('click', e => { e.stopPropagation(); dd.classList.toggle('open'); });
  menu.querySelectorAll('.dd-opt').forEach(o => o.addEventListener('click', () => { setMethodValue(o.dataset.id); dd.classList.remove('open'); }));
  document.addEventListener('click', e => { if (!dd.contains(e.target)) dd.classList.remove('open'); });
}
function ensureModal() {
  if (modal) return modal;
  modal = el('div', 'modal-bg');
  modal.innerHTML = `<div class="modal">
    <div class="modal-h"><h2 id="m-title">Пресет</h2><button class="modal-x" id="m-close">${svgIcon('x')}</button></div>
    <div class="modal-b">
      <label class="fld"><span>Название</span><input id="m-name" type="text" placeholder="Мой пресет"></label>
      <label class="fld"><span>Иконка</span><div class="icon-grid" id="m-icons"></div></label>
      <label class="fld"><span>Метод обхода</span>
        <input type="hidden" id="m-method">
        <div class="dd" id="m-method-dd">
          <button type="button" class="dd-btn" id="m-method-btn"><span class="dd-val">—</span><svg class="dd-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>
          <div class="dd-menu" id="m-method-menu"></div>
        </div>
      </label>
      <div class="fld-row">
        <label class="fld"><span>TTL (fake)</span><input id="m-ttl" type="number" min="1" max="64" value="8"></label>
        <label class="fld"><span>Точка реза</span><input id="m-split" type="text" value="sni"></label>
      </div>
      <div class="fld tgl-row">
        <button type="button" class="tgl" id="m-quic" role="switch" aria-checked="false"><span class="tgl-dot"></span></button>
        <div class="tgl-tx"><b>Блокировать QUIC (UDP 443)</b><small>YouTube и Google пойдут через TCP — обход надёжнее</small></div>
      </div>
      <label class="fld"><span>Сервисы (через запятую)</span><input id="m-svcs" type="text" placeholder="ChatGPT, Claude"></label>
      <label class="fld"><span>Домены (по одному на строку)</span><textarea id="m-domains" rows="7" placeholder="example.com"></textarea></label>
    </div>
    <div class="modal-f"><button class="btn-ghost" id="m-cancel">Отмена</button><button class="btn-primary" id="m-save">Сохранить</button></div>
  </div>`;
  document.body.appendChild(modal);
  buildMethodDD();
  const quicBtn = modal.querySelector('#m-quic');
  quicBtn.addEventListener('click', () => {
    quicBtn.classList.toggle('on');
    quicBtn.setAttribute('aria-checked', quicBtn.classList.contains('on'));
  });
  const iconBox = modal.querySelector('#m-icons');
  iconBox.innerHTML = ICON_CHOICES.map(k => `<button type="button" class="icon-opt" data-icon="${k}">${svgIcon(k)}</button>`).join('');
  iconBox.querySelectorAll('.icon-opt').forEach(b => b.addEventListener('click', () => {
    pickedIcon = b.dataset.icon;
    iconBox.querySelectorAll('.icon-opt').forEach(x => x.classList.toggle('sel', x === b));
  }));
  const close = () => modal.classList.remove('open');
  modal.querySelector('#m-close').addEventListener('click', close);
  modal.querySelector('#m-cancel').addEventListener('click', close);
  modal.addEventListener('click', e => { if (e.target === modal) close(); });
  modal.querySelector('#m-save').addEventListener('click', saveEditor);
  return modal;
}
let editingId = null;
let pickedIcon = 'globe';
function openEditor(id) {
  ensureModal();
  editingId = id;
  const p = id ? UI.presets.find(x => x.id === id) : null;
  modal.querySelector('#m-title').textContent = p ? `Настройка: ${p.name}` : 'Новый пресет';
  const nameI = modal.querySelector('#m-name');
  nameI.value = p?.name || ''; nameI.disabled = !!p?.builtin;
  pickedIcon = iconKey(p?.icon);
  modal.querySelectorAll('.icon-opt').forEach(x => x.classList.toggle('sel', x.dataset.icon === pickedIcon));
  setMethodValue(p?.strategy.method || 'fakeddisorder');
  modal.querySelector('#m-quic').classList.toggle('on', !!p?.strategy.block_quic);
  modal.querySelector('#m-quic').setAttribute('aria-checked', !!p?.strategy.block_quic);
  modal.querySelector('#m-ttl').value = p?.strategy.ttl ?? 8;
  modal.querySelector('#m-split').value = p?.strategy.split_pos || 'sni';
  modal.querySelector('#m-svcs').value = (p?.services||[]).join(', ');
  modal.querySelector('#m-domains').value = (p?.domains||[]).join('\n');
  modal.classList.add('open');
}
async function saveEditor() {
  const name = modal.querySelector('#m-name').value.trim();
  if (!name) { alert('Укажите название'); return; }
  const cur = editingId ? UI.presets.find(x => x.id === editingId) : null;
  const preset = {
    id: editingId || ('custom_' + Date.now().toString(36)),
    name,
    icon: pickedIcon,
    builtin: cur?.builtin || false,
    services: modal.querySelector('#m-svcs').value.split(',').map(s=>s.trim()).filter(Boolean),
    strategy: {
      method: modal.querySelector('#m-method').value,
      ttl: parseInt(modal.querySelector('#m-ttl').value)||8,
      split_pos: modal.querySelector('#m-split').value.trim()||'sni',
      block_quic: modal.querySelector('#m-quic').classList.contains('on'),
    },
    domains: modal.querySelector('#m-domains').value.split(/\n+/).map(s=>s.trim().toLowerCase()).filter(Boolean),
  };
  await B.savePreset(preset);
  modal.classList.remove('open');
  await reloadPresets();
  pushLog({ level:'ok', tag:'preset', msg:`сохранён пресет: ${name}` });
}

/* ===================================================================
   ДОМЕНЫ (вкладка «Списки доменов»)
   =================================================================== */
const hostList = $('host-list'), hostField = $('host-field'), hostAdd = $('host-add');

/* селектор активного пресета над полем ввода */
let presetBar;
function ensurePresetBar() {
  if (presetBar || !hostList) return;
  presetBar = el('div', 'preset-bar');
  hostList.parentElement.insertBefore(presetBar, hostList.parentElement.firstChild);
}
function methodBadge(s) {
  if (!s) return '';
  const m = (METHODS.find(x => x.id === s.method) || {}).name || s.method;
  const ttl = (s.method !== 'split' && s.ttl) ? ' · TTL ' + s.ttl : '';
  const q = s.block_quic ? ' · QUIC off' : '';
  return `${esc(m)}${ttl}${q}`;
}
function renderDomains() {
  if (!hostList) return;
  ensurePresetBar();
  if (presetBar) {
    presetBar.innerHTML = '<span class="pb-label">Список:</span>' +
      UI.presets.map(p => `<button class="pb-chip${p.id===UI.activeId?' on':''}" data-id="${p.id}">${presetIcon(p)} ${esc(p.name)}</button>`).join('');
    presetBar.querySelectorAll('.pb-chip').forEach(c =>
      c.addEventListener('click', () => selectPreset(c.dataset.id)));
  }
  const p = active();
  hostList.innerHTML = '';
  if (!p) return;
  const rules = (p.rules && p.rules.length) ? p.rules : [];
  // Встроенные группы правил (только просмотр — разная стратегия на каждую).
  for (const r of rules) {
    const grp = el('div', 'rule-grp');
    grp.innerHTML = `<div class="rule-h"><span class="rule-nm">${esc(r.name)}</span><span class="rule-st">${methodBadge(r.strategy)}</span><span class="rule-ct">${(r.domains||[]).length}</span></div>` +
      `<div class="rule-chips">${(r.domains||[]).map(d => `<span class="rchip">${esc(d)}</span>`).join('')}</div>`;
    hostList.appendChild(grp);
  }
  // Пользовательские домены (редактируемые).
  if (rules.length) {
    const hdr = el('div', 'rule-h cust');
    hdr.innerHTML = `<span class="rule-nm">Мои домены</span><span class="rule-st">доп. к спискам</span>`;
    hostList.appendChild(hdr);
  }
  const custom = p.domains || [];
  if (!custom.length) {
    hostList.appendChild(el('div','host-empty', rules.length ? 'Свои домены не добавлены — поле ниже.' : 'Список пуст — добавьте домен ниже.'));
    return;
  }
  for (const d of custom) {
    const row = el('div', 'host');
    row.innerHTML = `<div class="fav">${esc(d[0].toUpperCase())}</div><div class="dom">${esc(d)}</div><span class="st">обход активен</span><div class="x">${svgIcon('x')}</div>`;
    row.querySelector('.x').addEventListener('click', async () => {
      row.style.opacity = 0;
      await B.removeDomain(p.id, d);
      await reloadPresets();
      pushLog({ level:'inf', tag:'host', msg:'удалён из списка: ' + d });
    });
    hostList.appendChild(row);
  }
}
async function addHosts() {
  if (!hostField) return;
  const raw = hostField.value.trim();
  if (!raw) return;
  const p = active();
  // поддержка ввода нескольких доменов сразу (запятая / пробел / строка)
  const list = raw.split(/[\s,]+/).map(s => s.trim().toLowerCase()).filter(Boolean);
  for (const d of list) { await B.addDomain(p.id, d); pushLog({ level:'ok', tag:'host', msg:'добавлен домен: ' + d }); }
  hostField.value = '';
  await reloadPresets();
}
if (hostAdd) hostAdd.addEventListener('click', addHosts);
if (hostField) hostField.addEventListener('keydown', e => { if (e.key === 'Enter') addHosts(); });

/* ===================================================================
   ПАКЕТНЫЙ ПОТОК (canvas) — порт из прежнего скрипта
   =================================================================== */
(function flow() {
  const cv = $('flow'); if (!cv) return;
  const cx = cv.getContext('2d');
  function resize() { const r = cv.getBoundingClientRect(); cv.width = r.width*devicePixelRatio; cv.height = r.height*devicePixelRatio; cx.scale(devicePixelRatio, devicePixelRatio); }
  new ResizeObserver(() => { cx.setTransform(1,0,0,1,0,0); resize(); }).observe(cv);
  resize();
  const pkts = [];
  setInterval(() => { if (!UI.running) return; const h = cv.getBoundingClientRect().height; pkts.push({ x:18, y:h*0.5+(Math.random()-.5)*h*0.4, vx:1.1+Math.random()*0.7, frag:false, r:2+Math.random()*2 }); }, 90);
  function draw() {
    const w = cv.getBoundingClientRect().width, h = cv.getBoundingClientRect().height;
    cx.clearRect(0,0,w,h);
    const wallX = w*0.5; const on = UI.running;
    cx.font = '600 10px JetBrains Mono'; cx.textAlign = 'center';
    cx.fillStyle = 'rgba(228,232,237,.9)'; cx.fillText('YOU', 24, h*0.5-14);
    cx.fillStyle = on ? 'rgba(245,247,250,.9)' : 'rgba(120,126,135,.6)'; cx.fillText('NET', w-22, h*0.5-14);
    cx.strokeStyle = on ? 'rgba(160,166,175,.5)' : 'rgba(100,105,113,.55)'; cx.lineWidth = 2; cx.setLineDash([5,6]);
    cx.beginPath(); cx.moveTo(wallX,14); cx.lineTo(wallX,h-14); cx.stroke(); cx.setLineDash([]);
    cx.fillStyle = on ? 'rgba(170,176,185,.85)' : 'rgba(118,124,133,.85)'; cx.fillText('DPI', wallX, 12);
    [[24,'rgba(228,232,237,'],[w-22, on?'rgba(245,247,250,':'rgba(120,126,135,']].forEach(([x,c]) => { cx.beginPath(); cx.arc(x,h*0.5,6,0,7); cx.fillStyle=c+'.9)'; cx.fill(); });
    for (let i=pkts.length-1;i>=0;i--) { const p=pkts[i]; p.x += on?p.vx:p.vx*0.15;
      if (on && !p.frag && p.x>=wallX-2){ p.frag=true; p.r*=0.7; if(Math.random()>0.4) pkts.push({x:p.x,y:p.y+6,vx:p.vx*0.9,frag:true,r:p.r}); }
      const col = p.frag?'245,247,250':'150,158,168';
      cx.beginPath(); cx.arc(p.x,p.y,p.r,0,7); cx.fillStyle='rgba('+col+',.95)'; cx.shadowColor='rgba('+col+',.8)'; cx.shadowBlur=10; cx.fill(); cx.shadowBlur=0;
      cx.strokeStyle='rgba('+col+',.18)'; cx.lineWidth=p.r; cx.beginPath(); cx.moveTo(p.x-14,p.y); cx.lineTo(p.x,p.y); cx.stroke();
      if (p.x>w+10) pkts.splice(i,1);
    }
    requestAnimationFrame(draw);
  }
  draw();
})();

/* ===================================================================
   ОПРЕДЕЛЕНИЕ ПРОВАЙДЕРА + АВТО-ТЕСТ
   =================================================================== */
async function detectProvider() {
  const tryFetch = async (url, map) => {
    try {
      const ctrl = new AbortController();
      const tm = setTimeout(() => ctrl.abort(), 6000);
      const r = await fetch(url, { cache: 'no-store', signal: ctrl.signal });
      clearTimeout(tm);
      if (!r.ok) return null;
      return map(await r.json());
    } catch { return null; }
  };
  return (await tryFetch('https://ipwho.is/', j =>
            (j && j.success !== false) ? { isp: (j.connection && j.connection.isp) || j.org || j.isp, country: j.country, ip: j.ip } : null))
      || (await tryFetch('https://ipapi.co/json/', j =>
            (j && !j.error) ? { isp: j.org || j.asn_org, country: j.country_name, ip: j.ip } : null))
      || null;
}

/* Рекомендация стратегии по провайдеру (РФ-операторы). */
function recommendByIsp(isp) {
  const v = (isp || '').toLowerCase();
  const has = (...k) => k.some(x => v.includes(x));
  if (has('rostelecom','ростел','rt.ru'))            return { method:'fakeddisorder', ttl:6, block_quic:true,  note:'Ростелеком — глубокий DPI, нужен Fake+Disorder' };
  if (has('mts','мтс'))                               return { method:'fake',          ttl:3, block_quic:true,  note:'МТС — помогает fake с малым TTL' };
  if (has('beeline','вымпел','vimpel'))               return { method:'fakeddisorder', ttl:8, block_quic:true,  note:'Билайн — устойчивый Fake+Disorder' };
  if (has('megafon','мегафон'))                       return { method:'fake',          ttl:4, block_quic:true,  note:'МегаФон — fake + блокировка QUIC' };
  if (has('tele2','теле2'))                            return { method:'fakeddisorder', ttl:6, block_quic:true,  note:'Tele2 — Fake+Disorder' };
  if (has('er-telecom','dom.ru','дом.ru'))             return { method:'split',         ttl:8, block_quic:false, note:'Дом.ru — достаточно split' };
  if (has('yota','йота'))                              return { method:'fake',          ttl:3, block_quic:true,  note:'Yota — fake с малым TTL' };
  if (has('ttk','транстелеком'))                  return { method:'fakeddisorder', ttl:6, block_quic:false, note:'ТТК — Fake+Disorder' };
  return null;
}

async function showProviderOnLoad() {
  const prov = await detectProvider();
  if (!prov) return;
  pushLog({ level:'inf', tag:'net', msg: `провайдер: ${prov.isp||'?'}${prov.country?' · '+prov.country:''}${prov.ip?' · '+prov.ip:''}` });
  const anchor = $('hero-strat');
  const host = anchor && anchor.closest('.chip') && anchor.closest('.chip').parentElement;
  if (host && !$('prov-chip')) {
    const c = el('span', 'chip prov-chip');
    c.id = 'prov-chip';
    c.style.opacity = 0;
    c.innerHTML = `${svgIcon('antenna')}<b style="margin-left:7px">${esc(prov.isp||'сеть')}</b>${prov.country?' · '+esc(prov.country):''}`;
    host.appendChild(c);
    requestAnimationFrame(() => { c.style.transition = '.6s'; c.style.opacity = 1; });
  }
}

let atBg;
function ensureAutoTest() {
  if (atBg) return atBg;
  atBg = el('div', 'at-bg');
  atBg.innerHTML = `<div class="at-card">
    <div class="at-head">
      <div class="at-radar"><span></span><span></span><span></span><i>${svgIcon('antenna')}</i></div>
      <div><h2 id="at-title">Анализ соединения</h2><div class="at-sub" id="at-sub">Идёт проверка…</div></div>
    </div>
    <div class="at-steps" id="at-steps"></div>
    <div class="at-foot"><button class="btn-ghost" id="at-close">Закрыть</button><button class="btn-ghost" id="at-tune">Автоподбор</button><button class="btn-primary" id="at-apply" disabled>Применить</button></div>
  </div>`;
  document.body.appendChild(atBg);
  const close = () => atBg.classList.remove('open');
  atBg.querySelector('#at-close').addEventListener('click', close);
  atBg.querySelector('#at-tune').addEventListener('click', runWinwsTune);
  atBg.addEventListener('click', e => { if (e.target === atBg) close(); });
  return atBg;
}
async function runWinwsTune() {
  const p = active(); if (!p) return;
  const btn = $('at-tune'); if (btn) { btn.disabled = true; btn.textContent = 'Подбор…'; }
  if ($('at-sub')) $('at-sub').textContent = 'Автоподбор winws…';
  atStep('tune', 'Автоподбор winws', 'run', 'перебираем профили обхода (~15 с)');
  let rows = [];
  try { rows = await B.autotune(p.id); }
  catch (e) {
    atStep('tune', 'Автоподбор недоступен', 'bad', (e && e.message) || String(e));
    if (btn) { btn.disabled = false; btn.textContent = 'Автоподбор'; }
    return;
  }
  let best = null;
  rows.forEach((r, i) => {
    if (!best || r.ok > best.ok) best = r;
    atStep('tn' + i, r.name, r.applied ? 'ok' : (r.ok > 0 ? 'warn' : 'bad'),
      `${r.ok}/${r.total} доменов прошли${r.applied ? ' · выбрано' : ''}`);
  });
  atStep('tune', 'Автоподбор завершён', (best && best.ok > 0) ? 'ok' : 'warn',
    (best && best.ok > 0) ? `лучший профиль: ${best.name}` : 'ни один профиль не пробил DPI');
  await reloadPresets();
  pushLog({ level: (best && best.ok > 0) ? 'ok' : 'warn', tag: 'autotune', msg: (best && best.ok > 0) ? `winws: выбран ${best.name}` : 'winws: подбор не дал результата' });
  if (btn) { btn.disabled = false; btn.textContent = 'Автоподбор'; }
}

function atStep(id, label, state, extra) {
  const steps = $('at-steps');
  let row = document.getElementById('at-' + id);
  if (!row) { row = el('div', 'at-row'); row.id = 'at-' + id; steps.appendChild(row); }
  const ic = state === 'run' ? '<div class="at-spin"></div>'
           : state === 'ok'  ? svgIcon('check')
           : state === 'bad' ? svgIcon('x')
           : state === 'warn'? svgIcon('shuffle')
           : svgIcon('globe');
  row.className = 'at-row ' + (state || '');
  row.innerHTML = `<div class="at-ic">${ic}</div><div class="at-tx"><b>${esc(label)}</b>${extra?`<small>${esc(extra)}</small>`:''}</div>`;
}

let atBusy = false;
async function runAutoTest() {
  if (atBusy) return;
  atBusy = true;
  ensureAutoTest();
  atBg.classList.add('open');
  $('at-steps').innerHTML = '';
  const applyBtn = $('at-apply');
  applyBtn.disabled = true;
  $('at-title').textContent = 'Анализ соединения';

  // 1 — провайдер
  $('at-sub').textContent = 'Определяем провайдера…';
  atStep('prov', 'Определение провайдера', 'run');
  const prov = await detectProvider();
  if (prov) atStep('prov', prov.isp || 'Провайдер определён', 'ok', `${prov.country||''}${prov.ip?' · '+prov.ip:''}`);
  else atStep('prov', 'Провайдер не определён', 'warn', 'нет ответа от сервиса геолокации');

  // 2 — скан доменов
  $('at-sub').textContent = 'Проверяем доступность доменов…';
  const p = active();
  const domains = (p ? p.domains : []).slice(0, 6);
  atStep('scan', `Проверка доменов (${domains.length})`, 'run');
  let reports = [];
  try { reports = await B.probeDomains(domains); } catch (e) { reports = []; }
  const blocked = reports.filter(r => !r.direct_ok);
  const splitFix = reports.filter(r => !r.direct_ok && r.split_ok);
  atStep('scan', `Проверено доменов: ${reports.length}`, blocked.length ? 'bad' : 'ok',
    blocked.length ? `заблокировано: ${blocked.length} · помогает split: ${splitFix.length}` : 'всё доступно напрямую');
  reports.forEach((r, i) => atStep('d' + i, r.domain,
    r.direct_ok ? 'ok' : (r.split_ok ? 'warn' : 'bad'), r.detail));

  // 3 — рекомендация
  $('at-sub').textContent = 'Подбираем метод…';
  atStep('rec', 'Подбор стратегии', 'run');
  let method = 'fakeddisorder', ttl = 8, recTxt;
  if (!reports.length) { method = 'fakeddisorder'; recTxt = 'Не удалось проверить домены — оставляем устойчивый Fake+Disorder.'; }
  else if (!blocked.length) { method = null; recTxt = 'Блокировок не обнаружено — обход можно не включать.'; }
  else if (splitFix.length === blocked.length) { method = 'split'; recTxt = 'Достаточно лёгкого разбиения (Split).'; }
  else { method = 'fakeddisorder'; recTxt = 'Часть доменов требует глубокого обхода — выбран Fake+Disorder.'; }
  // Уточняем по провайдеру.
  const isprec = prov ? recommendByIsp(prov.isp) : null;
  let quic = !!(p && p.strategy && p.strategy.block_quic);
  if (method && isprec) {
    method = isprec.method; ttl = isprec.ttl; quic = !!isprec.block_quic;
    recTxt = isprec.note + '. ' + recTxt;
  }

  const mname = method ? ((METHODS.find(m => m.id === method) || {}).name || method) : '—';
  atStep('rec', method ? `Рекомендация: ${mname}${method!=='split'?' · TTL '+ttl:''}` : 'Обход не требуется', 'ok',
    recTxt + (prov ? ` (провайдер: ${prov.isp||'?'})` : ''));
  if (method && TAURI) atStep('tunehint', 'Можно за��устить «Автоподбор»', 'inf', 'перебор winws-профилей и выбор рабочего');
  $('at-sub').textContent = 'Готово';

  if (method && p) {
    applyBtn.disabled = false;
    applyBtn.onclick = async () => {
      await B.setStrategy(p.id, { method, ttl, split_pos: 'sni', block_quic: quic });
      await reloadPresets();
      pushLog({ level:'ok', tag:'autotest', msg: `применена стратегия ${mname} для «${p.name}»` });
      atBg.classList.remove('open');
    };
  } else {
    applyBtn.disabled = true;
  }
  atBusy = false;
}
if ($('auto-test')) $('auto-test').addEventListener('click', runAutoTest);

/* ===================================================================
   ИНИЦИАЛИЗАЦИЯ
   =================================================================== */
async function reloadPresets() {
  UI.presets = await B.listPresets();
  UI.activeId = await B.getActivePreset();
  renderPresets(); renderDomains(); syncHero();
}
async function init() {
  B.onStatus(renderStatus);
  B.onStats(renderStats);
  B.onLog(pushLog);
  await reloadPresets();
  const st = await B.getStatus();
  localUptime = st.uptime_secs || 0;
  renderStatus(st);
  pushLog({ level:'ok', tag:'engine', msg: TAURI ? 'SSBZapret готов · бэкенд подключён' : 'SSBZapret · режим превью (без движка)' });
  if (TAURI) {
    try {
      const ready = await B.engineReady();
      if (!ready) pushLog({ level:'warn', tag:'winws', msg:'winws.exe не найден в resources — обход не запустится. Скопируйте winws из дистрибутива Zapret.' });
    } catch {}
  }
  renderThemes();
  renderDns();
  showProviderOnLoad();
  if (TAURI) checkForUpdate();
}

// ───────────────────── Автообновление ─────────────────────
async function checkForUpdate() {
  try {
    const info = await B.checkUpdate();
    if (info && info.version) showUpdateBanner(info);
  } catch (e) {
    // Обновлятель ещё не настроен (нет ключа/релиза) — тихо игнорируем.
  }
}
function showUpdateBanner(info) {
  if (document.getElementById('upd-banner')) return;
  const b = el('div', 'upd-banner');
  b.id = 'upd-banner';
  const notes = info.notes ? info.notes.slice(0, 96) : 'Нажмите «Обновить» — приложение само скачает и установит новую версию.';
  b.innerHTML =
    `<div class="upd-ic">${svgIcon('shield')}</div>` +
    `<div class="upd-tx"><div class="upd-t">Доступно обновление · v${esc(info.version)}</div>` +
    `<div class="upd-s">${esc(notes)}</div></div>` +
    `<button class="upd-btn" id="upd-go">Обновить</button>` +
    `<button class="upd-x" id="upd-x">${svgIcon('x')}</button>`;
  document.body.appendChild(b);
  document.getElementById('upd-x').onclick = () => b.remove();
  document.getElementById('upd-go').onclick = async () => {
    const btn = document.getElementById('upd-go');
    btn.textContent = 'Загрузка…';
    btn.disabled = true;
    pushLog({ level:'inf', tag:'update', msg:'скачиваю обновление v' + info.version });
    try {
      await B.installUpdate(); // после установки приложение перезапустится
    } catch (e) {
      btn.textContent = 'Ошибка';
      btn.disabled = false;
      pushLog({ level:'err', tag:'update', msg:String(e) });
    }
  };
}
init();
