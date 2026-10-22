const tauri = window.__TAURI__;
const SVG = "http://www.w3.org/2000/svg";
const C = 160;
const R_IN = 46;
const R_OUT = 150;
const WHEELS = ["convert", "tools"];
const WHEEL_NAMES = { convert: "CONVERT", tools: "TOOLS" };
const LIFT = 5;

const rootEl = document.getElementById("root");
const wheelEl = document.getElementById("wheel");
const slicesEl = document.getElementById("slices");
const hubTop = document.getElementById("hub-top");
const hubBottom = document.getElementById("hub-bottom");
const pointerEl = document.getElementById("pointer");
const dotsEl = document.getElementById("dots");
const rippleEl = document.getElementById("ripple");
const fillsEl = document.getElementById("petal-fills");
const lightEl = document.querySelector("#caustic i");

// One hue per petal, in rainbow order so neighbours never clash. The Tools wheel starts further round.
const PALETTE = ["#7c5cff", "#3b82f6", "#06b6d4", "#10b981", "#84cc16", "#f59e0b", "#f97316", "#ec4899"];
const WHEEL_OFFSET = { convert: 0, tools: 4 };

function mix(hex, other, t) {
  const a = parseInt(hex.slice(1), 16), b = parseInt(other.slice(1), 16);
  const ch = (n, s) => Math.round(((n >> s) & 255) * (1 - t) + ((b >> s) & 255) * t);
  return `rgb(${ch(a, 16)}, ${ch(a, 8)}, ${ch(a, 0)})`;
}

function colourOf(i) {
  return PALETTE[(i + (WHEEL_OFFSET[state.wheel] || 0)) % PALETTE.length];
}

let state = { paths: [], actions: [], size: 0, mode: "drag", wheel: "convert", hot: -1, busy: false };

function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024, i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

function wheelActions(wheel = state.wheel) {
  return state.actions.filter((a) => a.wheel === wheel).slice(0, 8);
}

function availableWheels() {
  return WHEELS.filter((w) => wheelActions(w).length > 0);
}

const P_IN = 54;
const P_OUT = 142 - LIFT; // leaves room for the hovered petal to lift inside the disk
const P_ROUND = 8;

const ICONS = {
  image: '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="1.8"/><path d="M4 18l6-6 4 4 3-3 4 4"/>',
  doc: '<path d="M6 3h8l5 5v13H6z"/><path d="M14 3v5h5"/>',
  txt: '<path d="M6 3h8l5 5v13H6z"/><path d="M14 3v5h5"/><path d="M9 13h7M9 17h7"/>',
  crop: '<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>',
  compress: '<path d="M4 14h6v6"/><path d="M20 10h-6V4"/><path d="M10 14l-6 6"/><path d="M14 10l6-6"/>',
  tag: '<path d="M3 12V4h8l10 10-8 8z"/><circle cx="7.5" cy="7.5" r="1.5"/>',
  sliders: '<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="9" cy="6" r="2.2"/><circle cx="15" cy="12" r="2.2"/><circle cx="7" cy="18" r="2.2"/>',
  pen: '<path d="M4 20l4-1 11-11-3-3L5 16z"/><path d="M14 6l3 3"/>',
  frame: '<rect x="2.5" y="2.5" width="19" height="19" rx="4"/><rect x="7" y="7" width="10" height="10" rx="2"/>',
  redact: '<path d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6z"/><circle cx="12" cy="12" r="2.5"/><path d="M4 20L20 4"/>',
  tool: '<circle cx="12" cy="12" r="8"/><path d="M12 8v8M8 12h8"/>',
  word: '<path d="M6 3h8l5 5v13H6z"/><path d="M14 3v5h5"/><path d="M8.5 12l1.5 5 2-4 2 4 1.5-5"/>',
  merge: '<rect x="3" y="3" width="8" height="10" rx="1.5"/><rect x="13" y="3" width="8" height="10" rx="1.5"/><path d="M7 13v3a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2v-3M12 18v3"/>',
  split: '<rect x="7" y="2" width="10" height="8" rx="1.5"/><path d="M12 10v3M12 13l-5 3M12 13l5 3"/><rect x="3" y="16" width="7" height="6" rx="1.2"/><rect x="14" y="16" width="7" height="6" rx="1.2"/>',
  rotate: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>',
  passport: '<rect x="4" y="2.5" width="16" height="19" rx="2.5"/><circle cx="12" cy="10" r="3.2"/><path d="M7 18.5c1-2.6 3-3.8 5-3.8s4 1.2 5 3.8"/>',
};

function iconFor(id) {
  const map = {
    "convert.pdf": "doc", "convert.txt": "txt", "convert.docx": "word", "tool.crop": "crop", "tool.compress": "compress",
    "tool.pdfcompress": "compress", "tool.strip": "tag", "tool.edit": "sliders", "tool.annotate": "pen",
    "tool.addbg": "frame", "tool.redact": "redact", "tool.merge": "merge", "tool.split": "split",
    "tool.pdfrotate": "rotate", "tool.rotate": "rotate", "tool.passport": "passport",
  };
  return map[id] || (id.startsWith("convert.") ? "image" : "tool");
}

const pt = (a, r) => `${(C + r * Math.cos(a)).toFixed(2)} ${(C + r * Math.sin(a)).toFixed(2)}`;

function petalPath(a0, a1, n) {
  const ri = P_IN + P_ROUND, ro = P_OUT - P_ROUND;
  if (n === 1) {
    return `M${C + ro} ${C}A${ro} ${ro} 0 1 1 ${C - ro} ${C}A${ro} ${ro} 0 1 1 ${C + ro} ${C}Z` +
      `M${C + ri} ${C}A${ri} ${ri} 0 1 0 ${C - ri} ${C}A${ri} ${ri} 0 1 0 ${C + ri} ${C}Z`;
  }
  const pad = (r) => (P_ROUND + 3) / r;
  const b0o = a0 + pad(ro), b1o = a1 - pad(ro), b0i = a0 + pad(ri), b1i = a1 - pad(ri);
  const large = b1o - b0o > Math.PI ? 1 : 0;
  return `M${pt(b0o, ro)}A${ro} ${ro} 0 ${large} 1 ${pt(b1o, ro)}L${pt(b1i, ri)}A${ri} ${ri} 0 ${large} 0 ${pt(b0i, ri)}Z`;
}

// A thin highlight along the outer edge of a petal, like light catching the rim of glass.
function shinePath(a0, a1, n) {
  const r = P_OUT - 1;
  if (n === 1) return `M${pt(-2.4, r)}A${r} ${r} 0 0 1 ${pt(-0.7, r)}`;
  const span = a1 - a0, s0 = a0 + span * 0.22, s1 = a1 - span * 0.22;
  return `M${pt(s0, r)}A${r} ${r} 0 0 1 ${pt(s1, r)}`;
}

function setHub(top, bottom) {
  hubTop.textContent = top;
  hubBottom.textContent = bottom;
}

function restingHub() {
  if (!state.paths.length && state.mode === "drag") return setHub("ShiftNinja", "READING FILE");
  if (!state.paths.length) return setHub("Select", "A FILE");
  if (!state.actions.length) return setHub("No options", "FOR THIS FILE");
  const count = state.paths.length > 1 ? `${state.paths.length} files · ` : "";
  setHub(count + formatSize(state.size), WHEEL_NAMES[state.wheel]);
}

// The Tools wheel (crop, edit, passport…) is one more Shift away and easy to miss, so while nothing is
// hovered the hub alternates between this wheel's name and how to reach the other one.
let hintShown = false;
setInterval(() => {
  const wheels = availableWheels();
  if (state.busy || state.hot >= 0 || wheels.length < 2 || !rootEl.classList.contains("open")) {
    hintShown = false;
    return;
  }
  hintShown = !hintShown;
  const other = WHEEL_NAMES[wheels[(wheels.indexOf(state.wheel) + 1) % wheels.length]];
  hubBottom.textContent = !hintShown ? WHEEL_NAMES[state.wheel] : state.mode === "shortcut" ? `TAB: ${other}` : `⇧ ${other}`;
}, 1400);

function renderDots() {
  dotsEl.replaceChildren();
  const wheels = availableWheels();
  if (wheels.length < 2) return;
  const gap = 8, x0 = C - ((wheels.length - 1) * gap) / 2;
  wheels.forEach((w, i) => {
    const d = document.createElementNS(SVG, "circle");
    d.setAttribute("cx", x0 + i * gap);
    d.setAttribute("cy", 188);
    d.setAttribute("r", w === state.wheel ? 2.6 : 1.8);
    if (w === state.wheel) d.setAttribute("class", "on");
    dotsEl.append(d);
  });
}

function petalFill(i, c) {
  const grad = document.createElementNS(SVG, "linearGradient");
  grad.id = `pf-${i}`;
  for (const [k, v] of [["x1", 0], ["y1", 0], ["x2", 1], ["y2", 1]]) grad.setAttribute(k, v);
  for (const [offset, colour] of [[0, mix(c, "#ffffff", 0.35)], [0.55, c], [1, mix(c, "#000000", 0.18)]]) {
    const stop = document.createElementNS(SVG, "stop");
    stop.setAttribute("offset", offset);
    stop.setAttribute("stop-color", colour);
    grad.append(stop);
  }
  return grad;
}

function render() {
  slicesEl.replaceChildren();
  fillsEl.replaceChildren();
  const list = wheelActions();
  const n = list.length;
  list.forEach((action, i) => {
    const a0 = -Math.PI / 2 - Math.PI / n + (i * 2 * Math.PI) / n;
    const a1 = a0 + (2 * Math.PI) / n;
    const mid = (a0 + a1) / 2;
    const rMid = 98;
    const cx = n === 1 ? C : C + rMid * Math.cos(mid);
    const cy = n === 1 ? C - rMid : C + rMid * Math.sin(mid);
    const g = document.createElementNS(SVG, "g");
    g.setAttribute("class", "slice");
    g.dataset.index = i;
    g.style.setProperty("--i", i);
    const c = colourOf(i);
    g.style.setProperty("--c", c);
    g.style.setProperty("--ci", mix(c, "#000000", 0.25));
    g.style.setProperty("--ci-dark", mix(c, "#ffffff", 0.35));
    fillsEl.append(petalFill(i, c));
    g.style.setProperty("--dx", `${n === 1 ? 0 : (LIFT * Math.cos(mid)).toFixed(2)}px`);
    g.style.setProperty("--dy", `${n === 1 ? 0 : (LIFT * Math.sin(mid)).toFixed(2)}px`);
    const path = document.createElementNS(SVG, "path");
    path.setAttribute("class", "petal");
    path.setAttribute("d", petalPath(a0, a1, n));
    path.setAttribute("fill-rule", "evenodd");
    path.setAttribute("fill", `url(#pf-${i})`);
    path.setAttribute("stroke", `url(#pf-${i})`);
    const halo = document.createElementNS(SVG, "path");
    halo.setAttribute("class", "halo");
    halo.setAttribute("d", path.getAttribute("d"));
    const shine = document.createElementNS(SVG, "path");
    shine.setAttribute("class", "shine");
    shine.setAttribute("d", shinePath(a0, a1, n));
    const wrap = document.createElementNS(SVG, "g");
    wrap.setAttribute("class", "icon-wrap");
    const icon = document.createElementNS(SVG, "g");
    icon.setAttribute("class", "icon");
    icon.setAttribute("transform", `translate(${cx - 10} ${cy - 20}) scale(0.84)`);
    icon.innerHTML = ICONS[iconFor(action.id)];
    wrap.append(icon);
    const label = document.createElementNS(SVG, "text");
    label.setAttribute("x", cx);
    label.setAttribute("y", cy + 15);
    label.setAttribute("text-anchor", "middle");
    label.textContent = action.title.toUpperCase();
    g.append(halo, path, shine, wrap, label);
    if (state.mode === "shortcut") {
      const key = document.createElementNS(SVG, "text");
      key.setAttribute("class", "key");
      key.setAttribute("x", cx);
      key.setAttribute("y", cy + 28);
      key.setAttribute("text-anchor", "middle");
      key.textContent = i + 1;
      g.append(key);
    }
    slicesEl.append(g);
  });
  renderDots();
  paintHot();
}

// Hover only flips classes so the petals can spring out and back instead of being rebuilt.
function paintHot() {
  const list = wheelActions();
  slicesEl.querySelectorAll(".slice").forEach((g, i) => {
    g.classList.toggle("hot", i === state.hot);
    g.classList.toggle("dim", state.hot >= 0 && i !== state.hot);
  });
  if (state.hot >= 0) rootEl.style.setProperty("--hot", colourOf(state.hot));
  else if (!state.busy) rootEl.style.removeProperty("--hot");
  if (state.busy) return;
  if (state.hot >= 0 && list[state.hot]) setHub(list[state.hot].title, list[state.hot].panel ? "DROP TO OPEN" : "DROP TO RUN");
  else restingHub();
}

function setHot(i) {
  if (state.busy || state.hot === i) return;
  state.hot = i;
  paintHot();
}

function aim(x, y) {
  const dx = x - C, dy = y - C;
  lightEl.style.transform = `translate3d(${(x - 10).toFixed(1)}px, ${(y - 10).toFixed(1)}px, 0)`;
  const r = Math.hypot(dx, dy);
  rootEl.classList.toggle("hub-hover", r < R_IN);
  if (r < R_IN || r > R_OUT || state.busy) return rootEl.classList.remove("aiming");
  const a = Math.atan2(dy, dx), ri = 50;
  pointerEl.setAttribute("d", `M${pt(a - 0.32, ri)}A${ri} ${ri} 0 0 1 ${pt(a + 0.32, ri)}`);
  rootEl.classList.add("aiming");
}

function sliceAt(x, y) {
  const dx = x - C, dy = y - C;
  const r = Math.hypot(dx, dy);
  const n = wheelActions().length;
  if (r < R_IN || r > R_OUT || n === 0) return -1;
  let a = Math.atan2(dy, dx) + Math.PI / 2 + Math.PI / n;
  a = ((a % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  return Math.floor(a / ((2 * Math.PI) / n));
}

let hideTimer = 0;
function hide(animate = false) {
  clearTimeout(hideTimer);
  const finish = () => {
    rootEl.classList.remove("open", "busy", "done", "failed", "closing", "aiming", "hub-hover");
    state.hot = -1;
    state.busy = false;
    tauri.core.invoke("hide_wheel");
  };
  if (!animate) return finish();
  rootEl.classList.add("closing");
  hideTimer = setTimeout(finish, 140);
}

function shortName(path) {
  const name = path.split(/[\\/]/).pop();
  return name.length > 14 ? name.slice(0, 12) + "…" : name;
}

function ripple(x, y) {
  rippleEl.setAttribute("cx", x);
  rippleEl.setAttribute("cy", y);
  rippleEl.classList.remove("go");
  void rippleEl.getBoundingClientRect();
  rippleEl.classList.add("go");
}

function sliceCentre(i) {
  const n = wheelActions().length;
  if (n <= 1) return [C, C - 98];
  const mid = -Math.PI / 2 + (i * 2 * Math.PI) / n;
  return [C + 98 * Math.cos(mid), C + 98 * Math.sin(mid)];
}

async function runSlice(i, paths = state.paths) {
  const action = wheelActions()[i];
  if (state.busy) return;
  if (!action || !paths.length) return hide();
  ripple(...sliceCentre(i));
  if (action.panel) {
    try {
      await tauri.core.invoke("open_panel", { panel: action.panel, path: paths[0] });
    } catch (err) {
      console.error(err);
    }
    return hide();
  }
  state.hot = i;
  paintHot();
  state.busy = true;
  rootEl.classList.remove("aiming");
  rootEl.classList.add("busy");
  setHub(action.title, paths.length > 1 ? `${paths.length} FILES…` : "WORKING…");
  try {
    const created = await tauri.core.invoke("run_action", { id: action.id, paths });
    rootEl.classList.replace("busy", "done");
    setHub("✓ Saved", created.length > 1 ? `${created.length} FILES` : shortName(created[0]).toUpperCase());
    hideTimer = setTimeout(() => hide(true), 800);
  } catch (err) {
    rootEl.classList.replace("busy", "failed");
    setHub("✕ Failed", String(err).slice(0, 18).toUpperCase());
    console.error(err);
    hideTimer = setTimeout(() => hide(true), 1900);
  }
}

function cycleWheel(step = 1) {
  const wheels = availableWheels();
  if (wheels.length < 2 || state.busy) return;
  const at = wheels.indexOf(state.wheel);
  state.wheel = wheels[(at + step + wheels.length) % wheels.length];
  state.hot = -1;
  rootEl.classList.remove("swap-next", "swap-prev");
  void rootEl.getBoundingClientRect();
  rootEl.classList.add(step > 0 ? "swap-next" : "swap-prev");
  render();
}

tauri.event.listen("wheel:show", ({ payload }) => {
  clearTimeout(hideTimer);
  state = { ...payload, wheel: "convert", hot: -1, busy: false };
  state.wheel = availableWheels()[0] || "convert";
  document.body.classList.toggle("native-glass", !!payload.native_glass);
  rootEl.classList.remove("open", "busy", "done", "failed", "closing", "swap-next", "swap-prev", "aiming");
  render();
  void rootEl.getBoundingClientRect();
  rootEl.classList.add("open");
});

tauri.event.listen("wheel:cycle", () => cycleWheel());

// Windows sends where the button was released (its drag-and-drop into the wheel is unreliable); elsewhere the
// drop event does the work and this only tidies up a release outside the wheel.
tauri.event.listen("wheel:mouseup", ({ payload }) => {
  if (payload && state.mode === "drag" && !state.busy) {
    const i = sliceAt(payload.x, payload.y);
    return i >= 0 ? runSlice(i) : hide();
  }
  setTimeout(() => { if (!state.busy) hide(); }, 200);
});

tauri.event.listen("wheel:pointer", ({ payload }) => {
  if (!state.busy) track(payload.x, payload.y);
});

tauri.webview.getCurrentWebview().onDragDropEvent(({ payload }) => {
  const scale = state.drop_in_pixels ? window.devicePixelRatio || 1 : 1;
  const pos = payload.position ? { x: payload.position.x / scale, y: payload.position.y / scale } : null;
  if (payload.type === "enter" && payload.paths?.length && !state.paths.length) {
    state.paths = payload.paths;
    tauri.core.invoke("actions_for", { paths: payload.paths }).then(({ actions, size }) => {
      state.actions = actions;
      state.size = size;
      state.wheel = availableWheels()[0] || "convert";
      render();
    });
  }
  if (payload.type === "over" || payload.type === "enter") {
    if (pos) { aim(pos.x, pos.y); setHot(sliceAt(pos.x, pos.y)); }
  } else if (payload.type === "leave") {
    rootEl.classList.remove("aiming");
    setHot(-1);
  } else if (payload.type === "drop") {
    const i = pos ? sliceAt(pos.x, pos.y) : -1;
    if (i < 0) return hide();
    runSlice(i, payload.paths.length ? payload.paths : state.paths);
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") return hide();
  if (e.key === "Tab" || e.key === "Shift") { e.preventDefault(); return cycleWheel(e.shiftKey && e.key === "Tab" ? -1 : 1); }
  if (e.key === "ArrowRight" || e.key === "ArrowDown" || e.key === "ArrowLeft" || e.key === "ArrowUp") {
    e.preventDefault();
    const n = wheelActions().length;
    if (!n) return;
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1;
    return setHot(state.hot < 0 ? (step > 0 ? 0 : n - 1) : (state.hot + step + n) % n);
  }
  if (e.key === "Enter" && state.hot >= 0) return runSlice(state.hot);
  const n = Number(e.key);
  if (n >= 1 && n <= 8) runSlice(n - 1);
});

// Wheel-local coordinates, undoing the open animation's scale.
function local(e) {
  const r = wheelEl.getBoundingClientRect();
  return { x: ((e.clientX - r.left) / r.width) * 320, y: ((e.clientY - r.top) / r.height) * 320 };
}

// Pointer and drag-over events can fire far faster than the screen refreshes; handle at most one per frame.
let pending = null;
function track(x, y) {
  const first = !pending;
  pending = { x, y };
  if (first) requestAnimationFrame(() => {
    const p = pending;
    pending = null;
    if (!p) return;
    aim(p.x, p.y);
    setHot(sliceAt(p.x, p.y));
  });
}

wheelEl.addEventListener("pointermove", (e) => {
  const p = local(e);
  track(p.x, p.y);
});

wheelEl.addEventListener("pointerleave", () => {
  pending = null;
  rootEl.classList.remove("aiming", "hub-hover");
  setHot(-1);
});

let lastScroll = 0;
wheelEl.addEventListener("wheel", (e) => {
  e.preventDefault();
  if (Math.abs(e.deltaY) < 4 || e.timeStamp - lastScroll < 320) return;
  lastScroll = e.timeStamp;
  cycleWheel(e.deltaY > 0 ? 1 : -1);
}, { passive: false });

wheelEl.addEventListener("click", (e) => {
  const p = local(e);
  const i = sliceAt(p.x, p.y);
  if (i >= 0) return runSlice(i);
  if (e.target.id === "hub" || e.target === wheelEl || e.target.id === "sheen-disk") hide();
});

window.addEventListener("blur", () => { if (state.mode === "shortcut" && !state.busy) hide(); });
