const tauri = window.__TAURI__;
const PARAMS = window.__SHIFTNINJA_EDIT__ || {};
const $ = (id) => document.getElementById(id);
const view = $("view");
const ctx = view.getContext("2d");

const VIEW_MAX = 1600;

const INKS = ["#6f5be0", "#b4446c", "#3f8e2e", "#17330f", "#f4c45a", "#ffffff"];

const BACKGROUNDS = [
  ["#7c6cf0", "#4b55d8"],
  ["#f3e4cf", "#d9c19f"],
  ["#ffdbe6", "#ef8fb0"],
  ["#dff3d5", "#7fcf73"],
  ["#d6efff", "#8fc3ef"],
  ["#fff8f1"],
  ["#17330f"],
];

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

let img = null;
let src = null;
let doc = { w: 1, h: 1 };
let k = 1;
let mode = null;

function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  Object.assign(node, props);
  for (const c of [].concat(children)) node.append(c);
  return node;
}

function group(title, ...children) {
  return el("div", { className: "group" }, [title ? el("h2", { textContent: title }) : "", ...children]);
}

function chips(options, current, onPick) {
  const row = el("div", { className: "chips" });
  const buttons = options.map(([label, value]) => {
    const b = el("button", { type: "button", textContent: label });
    b.dataset.value = String(value);
    b.addEventListener("click", () => {
      buttons.forEach((x) => x.classList.toggle("on", x === b));
      onPick(value);
    });
    if (value === current) b.classList.add("on");
    return b;
  });
  row.append(...buttons);
  return row;
}

function actions(list) {
  return el("div", { className: "chips" }, list.map(([label, fn]) => {
    const b = el("button", { type: "button", textContent: label });
    b.addEventListener("click", fn);
    return b;
  }));
}

function slider(label, min, max, value, format, onInput) {
  const out = el("output", { textContent: format(value) });
  const input = el("input", { type: "range", min, max, step: 1, value, title: "Double-click to reset" });
  const fill = () => input.style.setProperty("--p", `${((Number(input.value) - min) / (max - min)) * 100}%`);
  const wrap = el("label", { className: "slider" }, [el("span", {}, [label, out]), input]);
  let settle = 0;
  input.addEventListener("input", () => {
    out.textContent = format(Number(input.value));
    fill();
    wrap.classList.add("moving");
    clearTimeout(settle);
    settle = setTimeout(() => wrap.classList.remove("moving"), 180);
    onInput(Number(input.value));
  });
  input.addEventListener("dblclick", () => {
    input.value = value;
    input.dispatchEvent(new Event("input"));
  });
  fill();
  return wrap;
}

// A soft ink splash from wherever a button was pressed.
document.addEventListener("pointerdown", (e) => {
  const b = e.target.closest?.("button");
  if (!b || b.disabled || b.classList.contains("swatch")) return;
  const r = b.getBoundingClientRect(), d = Math.max(r.width, r.height);
  const ink = el("span", { className: "ink" });
  Object.assign(ink.style, { width: `${d}px`, height: `${d}px`, left: `${e.clientX - r.left - d / 2}px`, top: `${e.clientY - r.top - d / 2}px` });
  b.append(ink);
  setTimeout(() => ink.remove(), 560);
});

function swatches(colors, current, onPick) {
  const row = el("div", { className: "chips" });
  const buttons = colors.map((c, i) => {
    const b = el("button", { type: "button", className: "swatch", title: c.join ? c.join(" → ") : c });
    b.style.background = Array.isArray(c) && c.length > 1 ? `linear-gradient(135deg, ${c[0]}, ${c[1]})` : [].concat(c)[0];
    b.addEventListener("click", () => {
      buttons.forEach((x) => x.classList.toggle("on", x === b));
      onPick(i);
    });
    if (i === current) b.classList.add("on");
    return b;
  });
  row.append(...buttons);
  return row;
}

function setStatus(text, error = false) {
  $("status").textContent = text;
  $("status").classList.toggle("error", error);
}

function setDoc(w, h) {
  doc = { w: Math.max(1, Math.round(w)), h: Math.max(1, Math.round(h)) };
  k = Math.min(1, VIEW_MAX / Math.max(doc.w, doc.h));
  view.width = Math.max(1, Math.round(doc.w * k));
  view.height = Math.max(1, Math.round(doc.h * k));
  fit();
}

function fit() {
  const stage = $("stage").getBoundingClientRect();
  const room = { w: Math.max(50, stage.width - 48), h: Math.max(50, stage.height - 48) };
  const s = Math.min(room.w / view.width, room.h / view.height);
  view.style.width = `${Math.round(view.width * s)}px`;
  view.style.height = `${Math.round(view.height * s)}px`;
  applyZoom();
}

// Zoom and pan sit on top of the fitted size as a CSS transform, so toDoc() keeps working unchanged.
const zoom = { z: 1, x: 0, y: 0 };
const ZOOM_MIN = 1, ZOOM_MAX = 8;

function applyZoom(smooth = false) {
  if (zoom.z <= ZOOM_MIN) Object.assign(zoom, { z: 1, x: 0, y: 0 });
  view.classList.toggle("smooth", smooth);
  view.style.transform = zoom.z === 1 ? "" : `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.z})`;
  $("zoom-level").textContent = `${Math.round(zoom.z * 100)}%`;
  $("stage").classList.toggle("pan", zoom.z > 1 && !mode?.down);
}

function zoomAt(factor, clientX, clientY, smooth = false) {
  const r = $("stage").getBoundingClientRect();
  const cx = (clientX ?? r.left + r.width / 2) - (r.left + r.width / 2);
  const cy = (clientY ?? r.top + r.height / 2) - (r.top + r.height / 2);
  const z = clamp(zoom.z * factor, ZOOM_MIN, ZOOM_MAX);
  zoom.x = cx - ((cx - zoom.x) * z) / zoom.z;
  zoom.y = cy - ((cy - zoom.y) * z) / zoom.z;
  zoom.z = z;
  applyZoom(smooth);
}

function resetZoom() {
  Object.assign(zoom, { z: 1, x: 0, y: 0 });
  applyZoom(true);
}

function toast(text) {
  const t = $("toast");
  t.querySelector("span").textContent = text;
  t.classList.remove("show");
  void t.offsetWidth;
  t.classList.add("show");
}

function toDoc(e) {
  const r = view.getBoundingClientRect();
  return { x: ((e.clientX - r.left) / r.width) * doc.w, y: ((e.clientY - r.top) / r.height) * doc.h };
}

function screenUnit() {
  const r = view.getBoundingClientRect();
  return r.width ? doc.w / r.width : 1;
}

// While dragging or sliding, repaint at most once per frame instead of on every event.
let frame = 0;
function renderSoon() {
  if (!frame) frame = requestAnimationFrame(() => { frame = 0; render(); });
}

// Each editor has its own colour.
const MODE_COLOURS = {
  crop: ["#6d5cf6", "#a78bfa"],
  edit: ["#0ea5a4", "#5eead4"],
  annotate: ["#e2448f", "#fb7aba"],
  addbg: ["#f2701c", "#fdba74"],
  redact: ["#4f63e6", "#93a5ff"],
};

function render() {
  if (frame) { cancelAnimationFrame(frame); frame = 0; }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, view.width, view.height);
  mode.paint(ctx, k, true);
  setStatus(mode.status());
}

// Undo/redo for the modes that collect marks (Annotate) or boxes (Redact).
const history = {
  redo: [],
  list() { return mode?.marks || mode?.boxes; },
  added() { this.redo = []; },
  undo() { const l = this.list(); if (l?.length) { this.redo.push(l.pop()); render(); } },
  again() { const l = this.list(); if (l && this.redo.length) { l.push(this.redo.pop()); render(); } },
  clear() { const l = this.list(); if (l) { l.length = 0; this.redo = []; render(); } },
  buttons() { return group("", actions([["Undo", () => this.undo()], ["Redo", () => this.again()], ["Clear", () => this.clear()]])); },
};

function unit() {
  return Math.max(src.width, src.height) / 800;
}

const Crop = {
  title: "Crop image",
  suffix: "Cropped",
  rect: null,
  ratio: null,
  drag: null,

  start() {
    setDoc(src.width, src.height);
    this.rect = { x: 0, y: 0, w: src.width, h: src.height };
    view.classList.add("cross");
    this.sizeLabel = el("span", { className: "dims" });
    return [
      group("Ratio", chips([["Free", null], ["1:1", 1], ["4:5", 4 / 5], ["9:16", 9 / 16], ["16:9", 16 / 9], ["Original", "original"]], null,
        (v) => this.setRatio(v))),
      group("Size", this.sizeLabel),
      el("p", { className: "hint", textContent: "Drag the box or its corners. Drag outside it to start a new box. Enter saves." }),
    ];
  },

  setRatio(v) {
    this.ratio = v === "original" ? src.width / src.height : v;
    if (!this.ratio) return render();
    const cx = this.rect.x + this.rect.w / 2, cy = this.rect.y + this.rect.h / 2;
    let w = src.width, h = w / this.ratio;
    if (h > src.height) { h = src.height; w = h * this.ratio; }
    this.rect = this.inBounds({ x: cx - w / 2, y: cy - h / 2, w, h });
    render();
  },

  inBounds(r) {
    let { x, y, w, h } = r;
    if (w > src.width) { w = src.width; if (this.ratio) h = w / this.ratio; }
    if (h > src.height) { h = src.height; if (this.ratio) w = h * this.ratio; }
    return { x: clamp(x, 0, src.width - w), y: clamp(y, 0, src.height - h), w, h };
  },

  handles() {
    const { x, y, w, h } = this.rect;
    return {
      nw: [x, y], n: [x + w / 2, y], ne: [x + w, y], e: [x + w, y + h / 2],
      se: [x + w, y + h], s: [x + w / 2, y + h], sw: [x, y + h], w: [x, y + h / 2],
    };
  },

  down(p) {
    const reach = 12 * screenUnit();
    for (const [name, [hx, hy]] of Object.entries(this.handles())) {
      if (Math.abs(p.x - hx) <= reach && Math.abs(p.y - hy) <= reach) {
        this.drag = { kind: "handle", name, p, start: { ...this.rect } };
        return;
      }
    }
    const r = this.rect;
    const inside = p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
    this.drag = inside ? { kind: "move", p, start: { ...r } } : { kind: "new", p };
  },

  move(p) {
    const d = this.drag;
    if (!d) return;
    const min = 8;
    if (d.kind === "move") {
      this.rect = this.inBounds({ ...d.start, x: d.start.x + p.x - d.p.x, y: d.start.y + p.y - d.p.y });
    } else if (d.kind === "new") {
      let w = Math.abs(p.x - d.p.x), h = Math.abs(p.y - d.p.y);
      if (this.ratio) h = w / this.ratio;
      const x = p.x < d.p.x ? d.p.x - w : d.p.x, y = p.y < d.p.y ? d.p.y - h : d.p.y;
      if (w >= min && h >= min) this.rect = this.inBounds(this.clipNew({ x, y, w, h }));
    } else {
      this.rect = this.resized(d, p);
    }
  },

  clipNew(r) {
    const x0 = clamp(r.x, 0, src.width), y0 = clamp(r.y, 0, src.height);
    let w = clamp(r.x + r.w, 0, src.width) - x0, h = clamp(r.y + r.h, 0, src.height) - y0;
    if (this.ratio) { if (w / h > this.ratio) w = h * this.ratio; else h = w / this.ratio; }
    return { x: x0, y: y0, w: Math.max(8, w), h: Math.max(8, h) };
  },

  resized(d, p) {
    const s = d.start, name = d.name, min = 8;
    let l = s.x, t = s.y, r = s.x + s.w, b = s.y + s.h;
    const dx = p.x - d.p.x, dy = p.y - d.p.y;
    if (name.includes("w")) l = clamp(l + dx, 0, r - min);
    if (name.includes("e")) r = clamp(r + dx, l + min, src.width);
    if (name.includes("n")) t = clamp(t + dy, 0, b - min);
    if (name.includes("s")) b = clamp(b + dy, t + min, src.height);
    if (!this.ratio) return { x: l, y: t, w: r - l, h: b - t };

    let w = r - l, h = b - t;
    if (name === "n" || name === "s") w = h * this.ratio; else h = w / this.ratio;
    const cx = s.x + s.w / 2, cy = s.y + s.h / 2;
    const roomW = name.includes("w") ? r : name.includes("e") ? src.width - l : 2 * Math.min(cx, src.width - cx);
    const roomH = name.includes("n") ? b : name.includes("s") ? src.height - t : 2 * Math.min(cy, src.height - cy);
    const scale = Math.min(1, roomW / w, roomH / h);
    w *= scale; h *= scale;
    const x = name.includes("w") ? r - w : name.includes("e") ? l : cx - w / 2;
    const y = name.includes("n") ? b - h : name.includes("s") ? t : cy - h / 2;
    return { x, y, w: Math.max(min, w), h: Math.max(min, h) };
  },

  up() { this.drag = null; },

  cursorAt(p) {
    const reach = 12 * screenUnit();
    const CURSORS = { nw: "nwse-resize", se: "nwse-resize", ne: "nesw-resize", sw: "nesw-resize", n: "ns-resize", s: "ns-resize", e: "ew-resize", w: "ew-resize" };
    for (const [name, [hx, hy]] of Object.entries(this.handles())) {
      if (Math.abs(p.x - hx) <= reach && Math.abs(p.y - hy) <= reach) return CURSORS[name];
    }
    const r = this.rect;
    return p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h ? "move" : "crosshair";
  },

  outputSize() { return { w: Math.round(this.rect.w), h: Math.round(this.rect.h) }; },

  paint(c, s, preview) {
    const r = this.rect;
    c.setTransform(s, 0, 0, s, 0, 0);
    if (!preview) {
      c.drawImage(img, r.x, r.y, r.w, r.h, 0, 0, r.w, r.h);
      return;
    }
    c.drawImage(img, 0, 0);
    const u = screenUnit();
    c.fillStyle = "rgba(23, 51, 15, 0.5)";
    c.beginPath();
    c.rect(0, 0, src.width, src.height);
    c.rect(r.x, r.y, r.w, r.h);
    c.fill("evenodd");
    c.strokeStyle = "rgba(255, 248, 241, 0.55)";
    c.lineWidth = u;
    c.beginPath();
    for (const f of [1 / 3, 2 / 3]) {
      c.moveTo(r.x + r.w * f, r.y); c.lineTo(r.x + r.w * f, r.y + r.h);
      c.moveTo(r.x, r.y + r.h * f); c.lineTo(r.x + r.w, r.y + r.h * f);
    }
    c.stroke();
    c.strokeStyle = "#fff8f1";
    c.lineWidth = 2 * u;
    c.strokeRect(r.x, r.y, r.w, r.h);
    const hs = 12 * u;
    for (const [hx, hy] of Object.values(this.handles())) {
      c.fillStyle = "#fff8f1";
      c.strokeStyle = MODE_COLOURS.crop[0];
      c.lineWidth = 2 * u;
      c.beginPath();
      c.roundRect(hx - hs / 2, hy - hs / 2, hs, hs, 3 * u);
      c.fill();
      c.stroke();
    }
    this.sizeLabel.textContent = `${Math.round(r.w)} × ${Math.round(r.h)} px`;
  },

  status() { return "Saves next to the original"; },
};

const Edit = {
  title: "Edit image",
  suffix: "Edited",
  rot: 0,
  flipH: false,
  flipV: false,
  bright: 0,
  contrast: 0,
  sat: 0,
  size: 1,

  start() {
    this.resize();
    const signed = (v) => (v > 0 ? `+${v}` : `${v}`);
    return [
      group("Rotate & flip", actions([
        ["⟲ Left", () => { this.rot = (this.rot + 270) % 360; this.resize(); }],
        ["⟳ Right", () => { this.rot = (this.rot + 90) % 360; this.resize(); }],
        ["⇋ Flip", () => { this.flipH = !this.flipH; render(); }],
        ["⇵ Flip", () => { this.flipV = !this.flipV; render(); }],
      ])),
      group("Adjust",
        slider("Brightness", -100, 100, 0, signed, (v) => { this.bright = v; renderSoon(); }),
        slider("Contrast", -100, 100, 0, signed, (v) => { this.contrast = v; renderSoon(); }),
        slider("Saturation", -100, 100, 0, signed, (v) => { this.sat = v; renderSoon(); })),
      group("Size", chips([["100%", 1], ["75%", 0.75], ["50%", 0.5], ["25%", 0.25]], 1, (v) => { this.size = v; this.resize(); })),
    ];
  },

  resize() {
    const turned = this.rot % 180 !== 0;
    const w = (turned ? src.height : src.width) * this.size;
    const h = (turned ? src.width : src.height) * this.size;
    setDoc(w, h);
    render();
  },

  outputSize() { return { w: doc.w, h: doc.h }; },

  paint(c, s) {
    const W = Math.round(doc.w * s), H = Math.round(doc.h * s);
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.translate(W / 2, H / 2);
    c.rotate((this.rot * Math.PI) / 180);
    c.scale(this.flipH ? -1 : 1, this.flipV ? -1 : 1);
    const turned = this.rot % 180 !== 0;
    const dw = turned ? H : W, dh = turned ? W : H;
    c.imageSmoothingQuality = "high";
    c.drawImage(img, -dw / 2, -dh / 2, dw, dh);
    c.setTransform(1, 0, 0, 1, 0, 0);
    if (this.bright || this.contrast || this.sat) {
      const data = c.getImageData(0, 0, W, H);
      adjustPixels(data.data, this.bright, this.contrast, this.sat);
      c.putImageData(data, 0, 0);
    }
  },

  status() { return `${doc.w} × ${doc.h} px`; },
};

function adjustPixels(px, bright, contrast, sat) {
  const b = bright * 2.55;
  const cv = contrast * 2.55;
  const f = (259 * (cv + 255)) / (255 * (259 - cv));
  const s = 1 + sat / 100;
  for (let i = 0; i < px.length; i += 4) {
    let r = f * (px[i] + b - 128) + 128;
    let g = f * (px[i + 1] + b - 128) + 128;
    let bl = f * (px[i + 2] + b - 128) + 128;
    const gray = 0.299 * r + 0.587 * g + 0.114 * bl;
    px[i] = gray + (r - gray) * s;
    px[i + 1] = gray + (g - gray) * s;
    px[i + 2] = gray + (bl - gray) * s;
  }
}

const Annotate = {
  title: "Annotate",
  suffix: "Annotated",
  tool: "arrow",
  color: INKS[1],
  thickness: 4,
  text: "Look here",
  marks: [],
  live: null,

  start() {
    setDoc(src.width, src.height);
    view.classList.add("cross");
    const textInput = el("input", { type: "text", value: this.text, placeholder: "Your text" });
    textInput.addEventListener("input", () => { this.text = textInput.value; });
    return [
      group("Tool", chips([["Pen", "pen"], ["Arrow", "arrow"], ["Box", "box"], ["Highlight", "highlight"], ["Text", "text"]], this.tool,
        (v) => { this.tool = v; })),
      group("Colour", this.colours()),
      group("Thickness", slider("", 1, 12, this.thickness, (v) => `${v} px`, (v) => { this.thickness = v; })),
      group("Text", textInput, el("p", { className: "hint", textContent: "With the Text tool, click where it should go." })),
      history.buttons(),
    ];
  },

  colours() {
    const custom = el("input", { type: "color", value: "#ff7a00", title: "Any colour" });
    const row = swatches(INKS, 1, (i) => { this.color = INKS[i]; custom.classList.remove("on"); });
    custom.addEventListener("input", () => {
      this.color = custom.value;
      row.querySelectorAll(".swatch").forEach((b) => b.classList.remove("on"));
      custom.classList.add("on");
    });
    row.append(custom);
    return row;
  },

  width() { return this.thickness * unit(); },

  down(p) {
    const base = { color: this.color, width: this.width() };
    if (this.tool === "text") {
      if (this.text.trim()) { this.marks.push({ ...base, type: "text", x: p.x, y: p.y, text: this.text.trim() }); history.added(); }
      return;
    }
    this.live = { ...base, type: this.tool, points: [p, p] };
  },

  move(p) {
    if (!this.live) return;
    if (this.live.type === "pen") this.live.points.push(p);
    else this.live.points[1] = p;
  },

  up() {
    const m = this.live;
    this.live = null;
    if (!m) return;
    const [a, b] = [m.points[0], m.points[m.points.length - 1]];
    if (m.type !== "pen" && Math.hypot(b.x - a.x, b.y - a.y) < 3 * unit()) return;
    this.marks.push(m);
    history.added();
  },

  outputSize() { return { w: src.width, h: src.height }; },

  paint(c, s) {
    c.setTransform(s, 0, 0, s, 0, 0);
    c.drawImage(img, 0, 0);
    for (const m of this.live ? [...this.marks, this.live] : this.marks) drawMark(c, m);
  },

  status() { return `${this.marks.length} mark${this.marks.length === 1 ? "" : "s"}`; },
};

function drawMark(c, m) {
  c.save();
  c.strokeStyle = m.color;
  c.fillStyle = m.color;
  c.lineWidth = m.width;
  c.lineCap = "round";
  c.lineJoin = "round";
  const [a, b] = [m.points?.[0], m.points?.[m.points.length - 1]];
  if (m.type === "pen") {
    c.beginPath();
    m.points.forEach((p, i) => (i ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
    c.stroke();
  } else if (m.type === "highlight") {
    c.globalAlpha = 0.38;
    c.lineWidth = m.width * 4;
    c.lineCap = "butt";
    c.beginPath(); c.moveTo(a.x, a.y); c.lineTo(b.x, b.y); c.stroke();
  } else if (m.type === "box") {
    c.strokeRect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
  } else if (m.type === "arrow") {
    const angle = Math.atan2(b.y - a.y, b.x - a.x);
    const head = Math.max(m.width * 4, 10);
    c.beginPath(); c.moveTo(a.x, a.y); c.lineTo(b.x, b.y);
    for (const side of [-1, 1]) {
      c.moveTo(b.x, b.y);
      c.lineTo(b.x - head * Math.cos(angle + side * 0.45), b.y - head * Math.sin(angle + side * 0.45));
    }
    c.stroke();
  } else if (m.type === "text") {
    c.font = `800 ${Math.max(12, m.width * 6)}px "Inter Tight", system-ui, sans-serif`;
    c.textBaseline = "middle";
    c.fillText(m.text, m.x, m.y);
  }
  c.restore();
}

const AddBg = {
  title: "Add background",
  suffix: "With Background",
  ext: "png",
  bg: 0,
  shape: 1,
  padding: 12,
  corners: 24,
  shadow: 60,

  start() {
    this.relayout();
    return [
      group("Background", swatches(BACKGROUNDS, this.bg, (i) => { this.bg = i; render(); })),
      group("Shape", chips([["Auto", 0], ["1:1", 1], ["4:5", 4 / 5], ["16:9", 16 / 9], ["9:16", 9 / 16]], this.shape,
        (v) => { this.shape = v; this.relayout(); })),
      group("Style",
        slider("Padding", 0, 30, this.padding, (v) => `${v}%`, (v) => { this.padding = v; this.relayout(); }),
        slider("Corners", 0, 60, this.corners, (v) => `${v} px`, (v) => { this.corners = v; renderSoon(); }),
        slider("Shadow", 0, 100, this.shadow, (v) => `${v}`, (v) => { this.shadow = v; renderSoon(); })),
    ];
  },

  relayout() {
    const pad = (this.padding / 100) * Math.max(src.width, src.height);
    let w = src.width + 2 * pad, h = src.height + 2 * pad;
    if (this.shape) {
      if (w / h < this.shape) w = h * this.shape; else h = w / this.shape;
    }
    setDoc(w, h);
    render();
  },

  outputSize() { return { w: doc.w, h: doc.h }; },

  paint(c, s) {
    c.setTransform(s, 0, 0, s, 0, 0);
    const stops = BACKGROUNDS[this.bg];
    if (stops.length > 1) {
      const g = c.createLinearGradient(0, 0, doc.w, doc.h);
      g.addColorStop(0, stops[0]);
      g.addColorStop(1, stops[1]);
      c.fillStyle = g;
    } else {
      c.fillStyle = stops[0];
    }
    c.fillRect(0, 0, doc.w, doc.h);

    const u = unit();
    const x = (doc.w - src.width) / 2, y = (doc.h - src.height) / 2;
    const radius = Math.min(this.corners * u, src.width / 2, src.height / 2);
    if (this.shadow) {
      c.save();
      c.shadowColor = `rgba(20, 10, 60, ${(0.45 * this.shadow) / 100})`;
      c.shadowBlur = this.shadow * u * 0.6 * s;
      c.shadowOffsetY = this.shadow * u * 0.25 * s;
      c.fillStyle = "#fff";
      c.beginPath();
      c.roundRect(x, y, src.width, src.height, radius);
      c.fill();
      c.restore();
    }
    c.save();
    c.beginPath();
    c.roundRect(x, y, src.width, src.height, radius);
    c.clip();
    c.drawImage(img, x, y);
    c.restore();
  },

  status() { return `${doc.w} × ${doc.h} px`; },
};

const Redact = {
  title: "Redact",
  suffix: "Redacted",
  style: "black",
  boxes: [],
  live: null,

  start() {
    setDoc(src.width, src.height);
    view.classList.add("cross");
    return [
      group("Style", chips([["Black box", "black"], ["Pixelate", "pixelate"]], this.style, (v) => { this.style = v; })),
      el("p", { className: "hint", textContent: "Drag over anything private. Black is the safest for passwords and account numbers." }),
      history.buttons(),
    ];
  },

  down(p) { this.live = { a: p, b: p, style: this.style }; },
  move(p) { if (this.live) this.live.b = p; },
  up() {
    const l = this.live;
    this.live = null;
    if (!l) return;
    const box = normalize(l.a, l.b);
    if (box.w >= 3 && box.h >= 3) { this.boxes.push({ ...box, style: l.style }); history.added(); }
  },

  outputSize() { return { w: src.width, h: src.height }; },

  paint(c, s, preview) {
    c.setTransform(s, 0, 0, s, 0, 0);
    c.drawImage(img, 0, 0);
    const all = this.live ? [...this.boxes, { ...normalize(this.live.a, this.live.b), style: this.live.style, live: true }] : this.boxes;
    for (const b of all) {
      if (b.w < 1 || b.h < 1) continue;
      if (b.style === "pixelate") pixelate(c, b);
      else { c.fillStyle = "#000"; c.fillRect(b.x, b.y, b.w, b.h); }
      if (preview && b.live) {
        c.strokeStyle = MODE_COLOURS.redact[0];
        c.lineWidth = 2 * screenUnit();
        c.strokeRect(b.x, b.y, b.w, b.h);
      }
    }
  },

  status() { return `${this.boxes.length} area${this.boxes.length === 1 ? "" : "s"} hidden`; },
};

function pixelate(c, b) {
  const block = Math.max(6, Math.round(Math.min(b.w, b.h) / 5));
  const tw = Math.max(1, Math.round(b.w / block)), th = Math.max(1, Math.round(b.h / block));
  const tiny = document.createElement("canvas");
  tiny.width = tw;
  tiny.height = th;
  tiny.getContext("2d").drawImage(img, b.x, b.y, b.w, b.h, 0, 0, tw, th);
  c.save();
  c.imageSmoothingEnabled = false;
  c.drawImage(tiny, 0, 0, tw, th, b.x, b.y, b.w, b.h);
  c.restore();
}

function normalize(a, b) {
  const x = clamp(Math.min(a.x, b.x), 0, src.width), y = clamp(Math.min(a.y, b.y), 0, src.height);
  return { x, y, w: clamp(Math.max(a.x, b.x), 0, src.width) - x, h: clamp(Math.max(a.y, b.y), 0, src.height) - y };
}

const MODES = { crop: Crop, edit: Edit, annotate: Annotate, addbg: AddBg, redact: Redact };

async function exportImage() {
  const { w, h } = mode.outputSize();
  const out = document.createElement("canvas");
  out.width = w;
  out.height = h;
  const oc = out.getContext("2d");
  const ext = mode.ext || src.ext;
  if (ext === "jpg") { oc.fillStyle = "#fff"; oc.fillRect(0, 0, w, h); }
  mode.paint(oc, 1, false);
  const blob = await new Promise((resolve) => out.toBlob(resolve, ext === "jpg" ? "image/jpeg" : "image/png", 0.92));
  const dataUrl = await new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.readAsDataURL(blob);
  });
  return { ext, data: String(dataUrl).split(",")[1] };
}

async function save() {
  const button = $("save");
  if (button.disabled) return;
  button.disabled = true;
  setStatus("Saving…");
  try {
    const { ext, data } = await exportImage();
    const saved = await tauri.core.invoke("save_edit", { path: PARAMS.path, suffix: mode.suffix, ext, data });
    const name = saved.split(/[\/]/).pop();
    setStatus(`Saved ${name}`);
    toast(`Saved ${name}`);
    setTimeout(close, 900);
  } catch (err) {
    setStatus(`Couldn't save: ${err}`, true);
    button.disabled = false;
  }
}

function close() {
  tauri.window.getCurrentWindow().close();
}

async function init() {
  document.documentElement.classList.toggle("native-glass", !!PARAMS.glass);
  mode = MODES[PARAMS.mode];
  if (!mode) return setStatus(`Unknown editor: ${PARAMS.mode}`, true);
  const [accent, accent2] = MODE_COLOURS[PARAMS.mode];
  document.documentElement.style.setProperty("--accent", accent);
  document.documentElement.style.setProperty("--accent-2", accent2);
  $("title").textContent = mode.title;
  document.title = mode.title;
  $("save").textContent = PARAMS.mode === "crop" ? "Crop" : "Save";
  try {
    src = await tauri.core.invoke("load_for_edit", { path: PARAMS.path });
  } catch (err) {
    $("loading").textContent = "";
    return setStatus(`Couldn't open this file: ${err}`, true);
  }
  $("filename").textContent = src.name + (/\.pdf$/i.test(src.name) ? " · page 1" : "");
  img = new Image();
  img.onload = () => {
    $("loading").remove();
    $("original").src = src.data_url;
    $("controls").append(...mode.start());
    $("save").disabled = false;
    render();
    view.classList.add("ready");
  };
  img.onerror = () => setStatus("Couldn't read this image", true);
  img.src = src.data_url;
}

// Pointer: left button edits; middle button, Space + drag, or any drag in a mode without drawing pans when zoomed in.
let dragging = false;
let panning = null;
let spaceHeld = false;

function startPan(e) {
  panning = { x: e.clientX, y: e.clientY, zx: zoom.x, zy: zoom.y };
  $("stage").classList.add("panning");
}

$("stage").addEventListener("pointerdown", (e) => {
  if (!img || e.target.closest("#toolbar")) return;
  const wantsPan = e.button === 1 || spaceHeld || (zoom.z > 1 && !mode?.down && e.button === 0);
  if (wantsPan) { e.preventDefault(); startPan(e); return; }
  if (e.target !== view || !mode?.down || e.button > 0) return;
  dragging = true;
  try { view.setPointerCapture(e.pointerId); } catch {  }
  mode.down(toDoc(e));
  render();
});
window.addEventListener("pointermove", (e) => {
  if (panning) {
    zoom.x = panning.zx + e.clientX - panning.x;
    zoom.y = panning.zy + e.clientY - panning.y;
    return applyZoom();
  }
  if (!dragging) {
    if (mode?.cursorAt && img && e.target === view && !spaceHeld) view.style.cursor = mode.cursorAt(toDoc(e));
    return;
  }
  if (!mode?.move) return;
  mode.move(toDoc(e));
  renderSoon();
});
window.addEventListener("pointerup", (e) => {
  if (panning) { panning = null; $("stage").classList.remove("panning"); return; }
  if (!dragging) return;
  dragging = false;
  mode.up?.(toDoc(e));
  render();
});

$("stage").addEventListener("wheel", (e) => {
  if (!img) return;
  e.preventDefault();
  zoomAt(Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0018)), e.clientX, e.clientY);
}, { passive: false });

$("zoom-in").addEventListener("click", () => zoomAt(1.4, undefined, undefined, true));
$("zoom-out").addEventListener("click", () => zoomAt(1 / 1.4, undefined, undefined, true));
$("zoom-level").addEventListener("click", resetZoom);
$("stage").addEventListener("dblclick", (e) => { if (!mode?.down && e.target === view) zoom.z > 1 ? resetZoom() : zoomAt(2, e.clientX, e.clientY, true); });

const compare = (on) => $("stage").classList.toggle("comparing", on && !!img);
$("compare").addEventListener("pointerdown", () => compare(true));
window.addEventListener("pointerup", () => compare(false));
$("compare").addEventListener("pointerleave", () => compare(false));

$("save").addEventListener("click", save);
$("cancel").addEventListener("click", close);
document.addEventListener("keydown", (e) => {
  if (e.target.tagName === "INPUT" && e.target.type === "text") return;
  const mod = e.metaKey || e.ctrlKey;
  if (e.key === "Escape") close();
  if (e.key === "Enter") save();
  if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? history.again() : history.undo(); }
  if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); history.again(); }
  if (e.key === "+" || e.key === "=") zoomAt(1.4, undefined, undefined, true);
  if (e.key === "-" || e.key === "_") zoomAt(1 / 1.4, undefined, undefined, true);
  if (e.key === "0") resetZoom();
  if (e.key === "\\") compare(true);
  if (e.key === " " && !spaceHeld) { spaceHeld = true; $("stage").classList.add("pan"); e.preventDefault(); }
});
document.addEventListener("keyup", (e) => {
  if (e.key === "\\") compare(false);
  if (e.key === " ") { spaceHeld = false; applyZoom(); }
});
window.addEventListener("resize", () => { if (img) fit(); });

init();
