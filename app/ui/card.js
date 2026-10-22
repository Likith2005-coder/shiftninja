const tauri = window.__TAURI__;
const $ = (id) => document.getElementById(id);

const HI = {
  tagline: "PDF, Word और फ़ोटो टूल",
  how: "कैसे इस्तेमाल करें",
  step1: "कोई भी PDF, Word फ़ाइल या फ़ोटो खींचना शुरू करें",
  step2: "<kbd>Shift</kbd> दबाएँ – एक पहिया खुलेगा",
  step3: "किसी पंखुड़ी पर छोड़ें। नई फ़ाइल असली फ़ाइल के पास सेव हो जाएगी",
  more: "टूल्स के लिए <kbd>Shift</kbd> फिर से दबाएँ (या स्क्रॉल करें): क्रॉप, कंप्रेस, पासपोर्ट फ़ोटो, मर्ज…",
  autostart: "Windows के साथ शुरू करें",
  "autostart-sub": "ताकि Shift पहिया हमेशा तैयार रहे",
  licences: "ओपन-सोर्स लाइसेंस",
};
const EN = {};
let lang = "en";
try { lang = localStorage.getItem("lang") || "en"; } catch {  }
let info = null;

function applyLang() {
  document.documentElement.lang = lang;
  document.querySelectorAll("[data-t]").forEach((el) => {
    const key = el.dataset.t;
    if (!(key in EN)) EN[key] = el.innerHTML;
    el.innerHTML = lang === "hi" && HI[key] ? HI[key] : EN[key];
  });
  $("lang").textContent = lang === "hi" ? "English" : "हिंदी";
  if (info) {
    $("shortcut-tip").textContent = lang === "hi"
      ? `या फ़ाइल चुनकर ${info.shortcut} दबाएँ।`
      : `Or select a file and press ${info.shortcut}.`;
    $("version").textContent = `${info.name} ${info.version}`;
  }
}

function toast(text) {
  const el = $("toast");
  el.textContent = text;
  el.classList.remove("show");
  void el.offsetWidth;
  el.classList.add("show");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove("show"), 1800);
}

async function init() {
  document.documentElement.classList.toggle("native-glass", !!window.__SHIFTNINJA__?.glass);
  info = await tauri.core.invoke("app_info");
  $("autostart").checked = await tauri.core.invoke("autostart_enabled");
  applyLang();
}

$("lang").addEventListener("click", () => {
  lang = lang === "hi" ? "en" : "hi";
  try { localStorage.setItem("lang", lang); } catch {  }
  applyLang();
});
$("autostart").addEventListener("change", async (e) => {
  try {
    e.target.checked = await tauri.core.invoke("set_autostart", { on: e.target.checked });
  } catch (err) {
    e.target.checked = !e.target.checked;
    toast(String(err));
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") tauri.window.getCurrentWindow().close();
});

init();
