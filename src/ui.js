// FIG Viewer frontend — GPU frame display + overlay rendering.
//
// Architecture:
//   - Rust renders the scene to an offscreen GPU texture and returns raw
//     RGBA bytes over Tauri's binary IPC channel (one call per displayed
//     frame, driven by a single requestAnimationFrame loop).
//   - This script blits those pixels into a 2D canvas, then draws text and
//     the selection outline on a transparent overlay canvas (browser fonts,
//     no font rasterization needed in Rust).

// ── DOM ──
const canvas = document.getElementById('c');
const ctx = canvas.getContext('2d');
const overlay = document.getElementById('overlay');
const octx = overlay.getContext('2d');
const wrap = document.getElementById('canvas-wrap');
const statusEl = document.getElementById('renderer-status');

const isTauri = !!window.__TAURI__?.core?.invoke;

async function invoke(cmd, args = {}) {
  if (!isTauri) { console.warn(`Tauri not available for ${cmd}`); return null; }
  try {
    return await window.__TAURI__.core.invoke(cmd, args);
  } catch (e) {
    console.error(`IPC ${cmd} failed:`, e);
    throw e;
  }
}

// ── State ──
let rendererReady = false;
let dirty = true;
let rendering = false;
let lastSentSize = { w: 0, h: 0, dpr: 0 };
let cam = { zoom: 1, pan_x: 0, pan_y: 0 };

// documents: id → { meta, page, selection, layerTree, flat, textItems, name }
const docs = new Map();
let activeDocId = null;

function activeDoc() { return activeDocId ? docs.get(activeDocId) : null; }
function markDirty() { dirty = true; }

// ── Renderer init ──
async function initRenderer() {
  if (rendererReady || !isTauri) {
    if (!isTauri) statusEl.textContent = 'Browser mode — open a .fig in the desktop app';
    return;
  }
  statusEl.textContent = 'GPU: initializing...';
  try {
    const w = wrap.clientWidth || 800;
    const h = wrap.clientHeight || 600;
    await invoke('init_renderer', { width: w, height: h });
    rendererReady = true;
    statusEl.textContent = 'GPU: ready';
    markDirty();
  } catch (e) {
    statusEl.textContent = 'GPU: error';
    showError('Renderer init failed: ' + (e?.message || e));
  }
}

// ── Render loop ──
function tick() {
  if (dirty && !rendering && rendererReady && activeDoc()) {
    rendering = true;
    dirty = false;
    renderFrame()
      .catch(e => console.warn('Render failed:', e))
      .finally(() => { rendering = false; });
  }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

async function renderFrame() {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.max(1, wrap.clientWidth);
  const h = Math.max(1, wrap.clientHeight);

  if (lastSentSize.w !== w || lastSentSize.h !== h || lastSentSize.dpr !== dpr) {
    await invoke('resize_viewport', { width: w, height: h, dpr });
    lastSentSize = { w, h, dpr };
  }

  const buf = await invoke('render_frame');
  if (!(buf instanceof ArrayBuffer)) return;

  const imgW = Math.floor(w * dpr);
  const imgH = Math.floor(h * dpr);
  if (canvas.width !== imgW) canvas.width = imgW;
  if (canvas.height !== imgH) canvas.height = imgH;
  if (canvas.style.width !== w + 'px') canvas.style.width = w + 'px';
  if (canvas.style.height !== h + 'px') canvas.style.height = h + 'px';

  const clamped = new Uint8ClampedArray(buf, 0, Math.min(buf.byteLength, imgW * imgH * 4));
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.putImageData(new ImageData(clamped, imgW, imgH), 0, 0);

  try {
    const c = await invoke('get_camera_state');
    if (c) cam = c;
    const lvl = document.getElementById('zoom-lvl');
    if (lvl) lvl.textContent = `${Math.round((cam.zoom || 1) * 100)}%`;
  } catch (_) { /* keep previous camera */ }

  drawOverlay(dpr);
}

// ── Overlay: text + selection ──
function drawOverlay(dpr) {
  const doc = activeDoc();
  const imgW = Math.floor(wrap.clientWidth * dpr);
  const imgH = Math.floor(wrap.clientHeight * dpr);
  if (overlay.width !== imgW) overlay.width = imgW;
  if (overlay.height !== imgH) overlay.height = imgH;
  if (overlay.style.width !== wrap.clientWidth + 'px') overlay.style.width = wrap.clientWidth + 'px';
  if (overlay.style.height !== wrap.clientHeight + 'px') overlay.style.height = wrap.clientHeight + 'px';
  octx.setTransform(dpr, 0, 0, dpr, 0, 0);
  octx.clearRect(0, 0, wrap.clientWidth, wrap.clientHeight);
  if (!doc) return;

  const zoom = cam.zoom || 1;
  const panX = cam.pan_x || 0;
  const panY = cam.pan_y || 0;

  // Text items
  for (const t of doc.textItems || []) {
    const size = t.font_size * zoom;
    if (size < 3) continue; // too small to be legible
    const x = t.x * zoom + panX;
    const y = t.y * zoom + panY;
    if (x > wrap.clientWidth || y > wrap.clientHeight) continue;
    if (x + t.w * zoom < 0 || y + t.h * zoom < 0) continue;

    const family = t.font_family || 'system-ui';
    const weight = Math.round(t.font_weight || 400);
    octx.font = `${weight} ${size}px "${family}", system-ui, sans-serif`;
    octx.fillStyle = `rgba(${Math.round(t.color[0] * 255)},${Math.round(t.color[1] * 255)},${Math.round(t.color[2] * 255)},${clamp01(t.color[3]) * clamp01(t.opacity)})`;
    octx.textBaseline = 'top';

    const lineHeight = (t.line_height && t.line_height > 0 ? t.line_height : t.font_size * 1.21) * zoom;
    const lines = String(t.characters).split(/\r?\n/);
    let ly = y + size * 0.08;
    for (const line of lines) {
      if (ly > wrap.clientHeight) break;
      octx.fillText(line, x, ly);
      ly += lineHeight;
    }
  }

  // Selection outline
  if (doc.selection) {
    const hit = doc.flat.find(n => n.id === doc.selection && n.bounds);
    if (hit) {
      const b = hit.bounds;
      const x = b.x * zoom + panX;
      const y = b.y * zoom + panY;
      const w = b.w * zoom;
      const h = b.h * zoom;
      octx.strokeStyle = '#18a0fb';
      octx.lineWidth = 1.5;
      octx.strokeRect(x - 1, y - 1, w + 2, h + 2);
      octx.fillStyle = '#18a0fb';
      for (const [hx, hy] of [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]) {
        octx.fillRect(hx - 3, hy - 3, 6, 6);
      }
    }
  }
}

function clamp01(v) { return Math.min(1, Math.max(0, Number(v) || 0)); }

// ── File loading ──
async function openDialog() {
  const tauri = window.__TAURI__;
  if (tauri?.dialog?.open) {
    try {
      const path = await tauri.dialog.open({ multiple: false, directory: false, filters: [{ name: 'Figma files', extensions: ['fig'] }] });
      if (typeof path === 'string' && path.length > 0) { await loadFromPath(path); }
    } catch (e) { console.error('Dialog failed:', e); }
    return;
  }
  const input = document.createElement('input');
  input.type = 'file'; input.accept = '.fig,application/zip';
  input.onchange = async () => { if (input.files?.[0]) await loadFromFile(input.files[0]); };
  input.click();
}

async function loadFromPath(path) {
  showLoading('Opening file...');
  try {
    const meta = await invoke('open_file', { path });
    onDocLoaded(meta, path.split(/[/\\]/).pop() || 'Untitled.fig');
  } catch (e) {
    showError('Failed to open: ' + (e?.message || e));
  }
}

async function loadFromFile(file) {
  const name = file.name || 'Untitled.fig';
  showLoading('Parsing file...');
  try {
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    const meta = await invoke('open_file_bytes', { data: bytes, name });
    onDocLoaded(meta, name);
  } catch (e) {
    showError('Failed to parse: ' + (e?.message || e));
  }
}

async function onDocLoaded(meta, tabName) {
  if (!meta || !Array.isArray(meta.pages) || meta.pages.length === 0) {
    showError('No pages found');
    return;
  }
  const doc = {
    meta,
    name: tabName || meta.file_name,
    page: 0,
    selection: null,
    layerTree: [],
    flat: [],
    textItems: [],
  };
  docs.set(meta.document_id, doc);
  activeDocId = meta.document_id;

  await refreshActiveDoc();
  renderTabs();
  markDirty();
}

async function refreshActiveDoc() {
  const doc = activeDoc();
  if (!doc) return;
  try {
    doc.layerTree = await invoke('get_layer_tree', { document_id: doc.meta.document_id, page_index: doc.page }) || [];
    doc.textItems = await invoke('get_page_text', { document_id: doc.meta.document_id, page_index: doc.page }) || [];
  } catch (_) {
    doc.layerTree = [];
    doc.textItems = [];
  }
  doc.flat = [];
  flatten(doc.layerTree, 0, doc.flat);
  renderSidebar(doc);
}

function flatten(nodes, depth, out) {
  for (const n of nodes) {
    out.push({
      id: n.id, name: n.name, node_type: n.node_type,
      depth, bounds: n.bounds, opacity: n.opacity,
    });
    if (n.children?.length) flatten(n.children, depth + 1, out);
  }
}

// ── Document switching / closing ──
async function switchTo(docId) {
  if (!docs.has(docId) || docId === activeDocId) return;
  const doc = docs.get(docId);
  try {
    await invoke('switch_document', { document_id: docId, page_index: doc.page });
  } catch (_) {}
  activeDocId = docId;
  await refreshActiveDoc();
  renderTabs();
  markDirty();
}

async function closeTab(docId) {
  docs.delete(docId);
  try { await invoke('close_file', { document_id: docId }); } catch (_) {}

  // The backend activates the first remaining tab; mirror that here.
  if (activeDocId === docId) {
    const remaining = [...docs.keys()];
    activeDocId = remaining.length ? remaining[0] : null;
    if (activeDocId) {
      const doc = docs.get(activeDocId);
      try { await invoke('set_page', { page_index: doc.page }); } catch (_) {}
      await refreshActiveDoc();
    }
  }
  renderTabs();
  if (!activeDoc()) { renderEmptyState(); clearCanvasDisplay(); }
  markDirty();
}

function clearCanvasDisplay() {
  const w = wrap.clientWidth || 1;
  const h = wrap.clientHeight || 1;
  canvas.width = w; canvas.height = h;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = '#e5e5e5';
  ctx.fillRect(0, 0, w, h);
  octx.clearRect(0, 0, w, h);
}

// ── Sidebar UI ──
function renderSidebar(doc) {
  const pagesEl = document.getElementById('pages');
  const layersEl = document.getElementById('layers');

  pagesEl.innerHTML = (doc.meta.pages || []).map((page, i) =>
    `<div class="page${i === doc.page ? ' active' : ''}" data-page="${i}">${esc(page.name)}</div>`).join('');
  pagesEl.querySelectorAll('.page').forEach(p => p.onclick = async () => {
    doc.page = parseInt(p.dataset.page);
    doc.selection = null;
    await invoke('select_node', { node_id: null }).catch(() => {});
    await invoke('set_page', { page_index: doc.page }).catch(() => {});
    await refreshActiveDoc();
    markDirty();
  });

  layersEl.innerHTML = doc.flat.map(node => layerItem(node)).join('');
  layersEl.querySelectorAll('.layer').forEach(ly => ly.onclick = async () => {
    await selectNode(ly.dataset.id, true);
  });
}

const LAYER_ICONS = { Frame: '[ ]', Group: '[ ]', Text: 'T', Rectangle: '[ ]', RoundedRectangle: '[ ]', Ellipse: 'O', Line: '/', Vector: '*', Component: '<>', Instance: '<>', Section: '#', BooleanGroup: '+', Canvas: 'P' };

function layerItem(node) {
  const doc = activeDoc();
  const selected = doc && doc.selection === node.id;
  return `<div class="layer${selected ? ' selected' : ''}" data-id="${esc(node.id)}" style="padding-left:${10 + node.depth * 14}px"><span class="icon">${LAYER_ICONS[node.node_type] || '*'}</span><span class="name">${esc(node.name)}</span></div>`;
}

async function selectNode(id, fitWhenFromPanel) {
  const doc = activeDoc();
  if (!doc) return;
  doc.selection = id || null;
  await invoke('select_node', { node_id: doc.selection }).catch(() => {});
  if (fitWhenFromPanel && doc.selection) {
    await invoke('fit_node', { node_id: doc.selection }).catch(() => {});
  }
  renderSidebar(doc);
  renderProps();
  markDirty();
}

async function renderProps() {
  const doc = activeDoc();
  const el = document.getElementById('props');
  if (!doc || !doc.selection) { el.innerHTML = ''; return; }
  try {
    const props = await invoke('get_node_properties', { document_id: doc.meta.document_id, node_id: doc.selection });
    if (!props) { el.innerHTML = ''; return; }
    let html = `<div class="pg"><div class="pl">Name</div><div class="pv">${esc(props.name)}</div></div>`;
    html += `<div class="pg"><div class="pl">Type</div><div class="pv">${esc(props.node_type)}</div></div>`;
    if (props.width != null) html += `<div class="pg"><div class="pl">Size</div><div class="pv">W:${f(props.width)} H:${f(props.height)}</div></div>`;
    if (props.x != null) html += `<div class="pg"><div class="pl">Position</div><div class="pv">X:${f(props.x)} Y:${f(props.y)}</div></div>`;
    html += `<div class="pg"><div class="pl">Opacity</div><div class="pv">${Math.round((props.opacity || 0) * 100)}%</div></div>`;
    if (props.fill_count > 0) html += `<div class="pg"><div class="pl">Fills</div><div class="pv">${props.fill_count} paint(s)</div></div>`;
    if (props.stroke_weight > 0) html += `<div class="pg"><div class="pl">Stroke</div><div class="pv">${f(props.stroke_weight)}px</div></div>`;
    if (props.corner_radius) html += `<div class="pg"><div class="pl">Corner</div><div class="pv">${f(props.corner_radius)}</div></div>`;
    if (props.font_size) html += `<div class="pg"><div class="pl">Font</div><div class="pv">${esc(props.font_family || '-')} ${props.font_weight} ${f(props.font_size)}px</div></div>`;
    el.innerHTML = html;
  } catch (_) { el.innerHTML = ''; }
}

// ── Tabs ──
function renderTabs() {
  const bar = document.getElementById('tab-bar');
  if (!docs.size) { renderEmptyState(); return; }
  bar.innerHTML = [...docs.entries()].map(([id, doc]) =>
    `<div class="tab${id === activeDocId ? ' active' : ''}" data-tab="${esc(id)}"><span class="name">${esc(doc.name)}</span><span class="close" data-close-tab="${esc(id)}">&times;</span></div>`
  ).join('');
}

function renderEmptyState() {
  document.getElementById('tab-bar').innerHTML =
    '<div class="empty" style="padding:8px;justify-content:flex-start;flex:1">Choose Open, press Ctrl+O, or drop a .fig file here</div>';
  document.getElementById('pages').innerHTML = '';
  document.getElementById('layers').innerHTML = '';
  document.getElementById('props').innerHTML = '';
}

// ── Camera control ──
async function zoomAt(screenX, screenY, newZoom) {
  await invoke('zoom_at', { screen_x: screenX, screen_y: screenY, zoom: newZoom }).catch(() => {});
  markDirty();
}

async function setZoomCentered(newZoom) {
  await invoke('set_zoom', { zoom: newZoom }).catch(() => {});
  markDirty();
}

async function fitPage() {
  await invoke('fit_page', { padding: 48 }).catch(() => {});
  markDirty();
}

async function fitSelection() {
  const doc = activeDoc();
  if (doc && doc.selection) {
    await invoke('fit_node', { node_id: doc.selection }).catch(() => {});
  } else {
    await fitPage();
  }
}

// ── UI helpers ──
function showLoading(msg) {
  document.getElementById('tab-bar').innerHTML =
    `<div class="tab active" style="color:var(--ac)">${esc(msg)}</div>`;
}
function showError(msg) {
  document.getElementById('tab-bar').innerHTML =
    `<div class="tab active" style="color:var(--danger)">${esc(msg)} <span class="close" data-dismiss>&times;</span></div>`;
}
function esc(v) { return String(v ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }
function f(v) { return Math.round((Number(v) || 0) * 100) / 100; }
function openGuide() { document.getElementById('guide-backdrop').classList.add('open'); document.getElementById('guide-close').focus(); }
function closeGuide() { document.getElementById('guide-backdrop').classList.remove('open'); }

// ── Event bindings ──
document.getElementById('open-file').onclick = openDialog;
document.getElementById('fit').onclick = fitPage;
document.getElementById('zin').onclick = () => zoomAt(wrap.clientWidth / 2, wrap.clientHeight / 2, (cam.zoom || 1) * 1.25);
document.getElementById('zout').onclick = () => zoomAt(wrap.clientWidth / 2, wrap.clientHeight / 2, (cam.zoom || 1) * 0.8);
document.getElementById('z100').onclick = () => setZoomCentered(1);
document.getElementById('guide-button').onclick = openGuide;
document.getElementById('guide-close').onclick = closeGuide;
document.getElementById('guide-backdrop').onclick = e => { if (e.target.id === 'guide-backdrop') closeGuide(); };

document.getElementById('tab-bar').addEventListener('click', async e => {
  const dismiss = e.target.closest('[data-dismiss]');
  if (dismiss) { dismiss.parentElement.remove(); return; }
  const close = e.target.closest('[data-close-tab]');
  if (close) { await closeTab(close.getAttribute('data-close-tab')); return; }
  const tab = e.target.closest('[data-tab]');
  if (tab) await switchTo(tab.getAttribute('data-tab'));
});

// Wheel zoom
wrap.addEventListener('wheel', e => {
  e.preventDefault();
  const rect = wrap.getBoundingClientRect();
  zoomAt(e.clientX - rect.left, e.clientY - rect.top, (cam.zoom || 1) * (e.deltaY < 0 ? 1.12 : 0.89));
}, { passive: false });

wrap.addEventListener('contextmenu', e => e.preventDefault());

// Panning + click select
let spaceDown = false;
let panGesture = null;

wrap.addEventListener('pointerdown', async e => {
  if (e.button === 1 || e.button === 2 || (e.button === 0 && spaceDown)) {
    e.preventDefault();
    panGesture = { pointerId: e.pointerId, x: e.clientX, y: e.clientY };
    wrap.classList.add('panning');
    wrap.setPointerCapture(e.pointerId);
    return;
  }
  if (e.button === 0) {
    // Click-select via backend hit test.
    const doc = activeDoc();
    if (!doc) return;
    const rect = wrap.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const wx = (sx - (cam.pan_x || 0)) / (cam.zoom || 1);
    const wy = (sy - (cam.pan_y || 0)) / (cam.zoom || 1);
    try {
      const id = await invoke('hit_test', { document_id: doc.meta.document_id, page_index: doc.page, worldX: wx, worldY: wy });
      await selectNode(id || null, false);
    } catch (_) {}
  }
});
wrap.addEventListener('pointermove', e => {
  if (!panGesture || panGesture.pointerId !== e.pointerId) return;
  const dx = e.clientX - panGesture.x;
  const dy = e.clientY - panGesture.y;
  panGesture.x = e.clientX; panGesture.y = e.clientY;
  invoke('pan_camera', { dx, dy }).catch(() => {});
  markDirty();
});
const endPan = e => { if (panGesture?.pointerId === e.pointerId) { panGesture = null; wrap.classList.remove('panning'); } };
wrap.addEventListener('pointerup', endPan);
wrap.addEventListener('pointercancel', endPan);

// Keyboard
document.addEventListener('keydown', e => {
  if (e.target && ['INPUT', 'TEXTAREA'].includes(e.target.tagName)) return;
  if (e.code === 'Space') { spaceDown = true; e.preventDefault(); }
  if (e.key === '?' || (e.shiftKey && e.key === '/')) { e.preventDefault(); openGuide(); return; }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') { e.preventDefault(); openDialog(); return; }
  if (e.key === 'Escape') {
    if (document.getElementById('guide-backdrop').classList.contains('open')) closeGuide();
    else selectNode(null, false);
    return;
  }
  if (e.key === '+' || e.key === '=') { zoomAt(wrap.clientWidth / 2, wrap.clientHeight / 2, (cam.zoom || 1) * 1.25); return; }
  if (e.key === '-') { zoomAt(wrap.clientWidth / 2, wrap.clientHeight / 2, (cam.zoom || 1) * 0.8); return; }
  if (e.key === '0') { fitPage(); return; }
  if (e.key === '1') { setZoomCentered(1); return; }
  if (e.key === '2') { fitSelection(); return; }
});
document.addEventListener('keyup', e => { if (e.code === 'Space') spaceDown = false; });

// Resize — just flag dirty; renderFrame sends resize_viewport when dims change.
new ResizeObserver(() => markDirty()).observe(wrap);

// Drag & drop
if (window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen('tauri://drag-drop', async e => {
    const paths = e.payload?.paths;
    const path = Array.isArray(paths) ? paths.find(p => typeof p === 'string' && p.toLowerCase().endsWith('.fig')) : null;
    if (path) await loadFromPath(path);
    else if (Array.isArray(paths) && paths.length) showError('Drop a .fig file here');
  }).catch(() => {});
} else {
  document.body.addEventListener('dragover', e => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; });
  document.body.addEventListener('drop', async e => {
    e.preventDefault();
    const file = e.dataTransfer.files[0];
    if (file?.name?.toLowerCase().endsWith('.fig')) await loadFromFile(file);
    else showError('Drop a .fig file here');
  });
}

// Startup
renderEmptyState();
if (isTauri) {
  initRenderer().then(() => {
    invoke('take_startup_path').then(async path => {
      if (typeof path === 'string' && path.length) await loadFromPath(path);
    }).catch(() => {});
  });
}
