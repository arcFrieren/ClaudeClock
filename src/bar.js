/* Barra overlay ClaudeClock — lógica de render y estados.
   Reglas de rendimiento (SPEC §9): un solo timer de 1 s, textContent solo si cambió,
   sin reconstrucción de DOM en updates, render pausado cuando la barra está oculta. */
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const THEME_CLASS = { term:"d01", geist:"d02", ticks:"d06", cond:"d07", hyper:"d08", blue:"d10" };
const METERS = ["s","w","f"];

let cfg = null;
let usage = null;       // { s,w,f, reset_s, reset_w, act_s, act_w, act_f, ts }
let refs = null;        // referencias cacheadas del DOM del tema activo
let theme = "geist";
let cdCache = "";
let timer = null;
let conn = { connected: true, demo: true }; // estado de sesión (SPEC §3.5)
let statusKnown = false; // evita un RECONECTAR falso antes del primer poll

const bar = document.getElementById("bar");

/* ---------- markup por tema (se construye UNA vez por cambio de tema) ---------- */
function segHtml(m, label, inner){
  return `<span class="seg ${m}" data-m="${m}">${label}${inner}<span class="val"></span></span>`;
}
const BUILDERS = {
  d01(){ // Terminal
    bar.innerHTML = METERS.map(m=>segHtml(m,
      `<span class="lbl">${name(m)}</span>`, `<span class="blocks"></span>`)).join("")
      + `<span class="cd"></span>`;
  },
  d02(){ // Geist minimal
    bar.innerHTML = METERS.map(m=>segHtml(m,
      `<span class="lbl">${name(m)}</span>`, `<span class="track"><span class="fillb"></span></span>`)).join("")
      + `<span class="cd"></span>`;
  },
  d06(){ // Data ticks
    const ticks = `<span class="ticks">${"<i></i>".repeat(10)}</span>`;
    bar.innerHTML = METERS.map(m=>segHtml(m, `<span class="lbl">${name(m)}</span>`, ticks)).join("")
      + `<span class="cd"></span>`;
  },
  d07(){ // Condensada triple
    bar.innerHTML =
      `<span class="stack">` +
        METERS.map(m=>`<span class="track seg ${m}" data-stk="${m}"><span class="fillb"></span></span>`).join("") +
      `</span>` +
      METERS.map(m=>segHtml(m, `<span class="lbl">${name(m)}</span>`, ``)).join("") +
      `<span class="cd"></span>`;
  },
  d08(){ // Hyperlegible
    bar.innerHTML = METERS.map((m,i)=>segHtml(m, `<span class="lbl">${name(m).toUpperCase()}</span>`, ``)
      + (i<2?`<span class="pipe">|</span>`:``)).join("")
      + `<span class="cd"></span>`;
  },
  d10(){ // Blueprint
    const short = {s:"Ses", w:"Sem", f:"Fab"};
    bar.innerHTML = METERS.map(m=>segHtml(m,
      `<span class="lbl">${short[m]}</span>`, `<span class="track"><span class="fillb"></span></span>`)).join("")
      + `<span class="cd"></span>`;
  }
};
function name(m){ return m==="s" ? "Sesión" : m==="w" ? "Semanal" : "Fable"; }

function applyTheme(){
  const cls = THEME_CLASS[theme] || "d02";
  bar.className = "bar " + cls + (cls==="d10" ? "" : " stateful");
  BUILDERS[cls]();
  refs = {};
  METERS.forEach(m=>{
    const seg = bar.querySelector(`.seg[data-m="${m}"]`);
    refs[m] = {
      seg,
      stk: bar.querySelector(`[data-stk="${m}"]`),        // solo d07
      val: seg.querySelector(".val"),
      fill: (seg.querySelector(".fillb")) || (bar.querySelector(`[data-stk="${m}"] .fillb`)),
      blocks: seg.querySelector(".blocks"),
      ticks: seg.querySelectorAll(".ticks i"),
      pct: -1
    };
  });
  refs.cd = bar.querySelector(".cd");
  cdCache = "";
  if(usage) render(usage, false);
  renderCountdown(true);
}

/* ---------- render de datos ---------- */
function render(u, pulse){
  const cls = THEME_CLASS[theme];
  METERS.forEach(m=>{
    const r = refs[m], pct = Math.round(u[m]);
    if(pct !== r.pct){
      r.pct = pct;
      const t = cls==="d02" ? String(pct) : pct + "%";
      r.val.textContent = t;
      if(r.fill)   r.fill.style.width = pct + "%";
      if(r.blocks) r.blocks.textContent = "▓".repeat(Math.round(pct/10)) + "░".repeat(10-Math.round(pct/10));
      if(r.ticks.length) r.ticks.forEach((i,idx)=>i.classList.toggle("on", idx < Math.round(pct/10)));
      r.seg.classList.toggle("warn",   pct>=80 && pct<90);
      r.seg.classList.toggle("danger", pct>=90);
      if(r.stk){
        r.stk.classList.toggle("warn",   pct>=80 && pct<90);
        r.stk.classList.toggle("danger", pct>=90);
      }
    }
  });
  // máquina de estados: reposo / actividad / actividad con Fable
  const st = u.act_f ? "st-fab" : (u.act_s || u.act_w) ? "st-use" : "st-idle";
  document.body.classList.remove("st-idle","st-use","st-fab");
  document.body.classList.add(st);
  // pulso de 1 s en cada actualización con actividad (Blueprint se conserva intacto)
  if(pulse && st!=="st-idle" && cls!=="d10"){
    const sel = st==="st-fab" ? ".seg" : ".seg.s, .seg.w";
    bar.querySelectorAll(sel).forEach(el=>{
      el.classList.remove("flash");
      void el.offsetWidth;
      el.classList.add("flash");
    });
  }
}

/* ---------- cuenta regresiva (un solo timer de 1 s) ---------- */
const p2 = n => String(n).padStart(2,"0");
function renderCountdown(force){
  if(!refs) return;
  let t;
  if(statusKnown && !conn.connected && !conn.demo){
    t = "RECONECTAR"; // sesión caída o endpoint cambiado (SPEC §3.5)
  } else {
    if(!usage) return;
    const now = Math.floor(Date.now()/1000);
    const left = Math.max(0, usage.reset_s - now);
    const h = Math.floor(left/3600), mn = Math.floor(left%3600/60);
    // formato unificado en todos los temas, como Hyperlegible
    t = `RESET ${p2(h)}:${p2(mn)}`;
  }
  refs.cd.classList.toggle("alert", t === "RECONECTAR");
  if(force || t !== cdCache){ cdCache = t; refs.cd.textContent = t; fitCompact(); }
}
function startTimer(){
  if(timer) return;
  timer = setInterval(()=>renderCountdown(false), 1000);
}
function stopTimer(){ clearInterval(timer); timer = null; }

/* ---------- ancho compacto: encoger la ventana al contenido real ---------- */
let lastW = 0;
function fitCompact(){
  if(!cfg || !cfg.alto_modo.startsWith("compacto")) return;
  // scrollWidth mide el contenido real aunque la ventana ya esté encogida
  const w = bar.scrollWidth + 2;
  if(Math.abs(w - lastW) > 8){
    lastW = w;
    invoke("resize_bar", { width: w });
  }
}

/* ---------- config ---------- */
function applyConfig(c){
  cfg = c;
  document.body.classList.toggle("nobg", c.fondo === "sin");
  document.body.classList.toggle("compact", c.alto_modo.startsWith("compacto"));
  document.body.classList.remove("pos-izq","pos-med","pos-der");
  document.body.classList.add("pos-" + ((c.posicion || "izq").split("-")[0]));
  if(c.tema !== theme || !refs){ theme = c.tema; applyTheme(); }
  lastW = 0; // forzar re-medición al cambiar modo/tema
  fitCompact();
}

/* ---------- interacción ----------
   La barra es un overlay pasivo: NO da acceso al programa (se entra por el
   icono de la bandeja del sistema). Solo se anula el menú contextual nativo. */
document.addEventListener("contextmenu", e=>e.preventDefault());

/* pausar todo render cuando la barra está oculta (SPEC §9.4) */
document.addEventListener("visibilitychange", ()=>{
  if(document.hidden) stopTimer();
  else { renderCountdown(true); startTimer(); }
});

/* ---------- arranque ---------- */
(async ()=>{
  const boot = await invoke("boot");
  conn = { connected: boot.connected, demo: boot.demo };
  statusKnown = boot.demo || boot.usage !== null;
  applyConfig(boot.config);
  if(boot.usage){ usage = boot.usage; render(usage, false); }
  renderCountdown(true);
  startTimer();
  await listen("usage", e=>{
    usage = e.payload;
    if(!document.hidden){ render(usage, true); renderCountdown(false); fitCompact(); }
  });
  await listen("config-changed", e=>applyConfig(e.payload));
  await listen("status", e=>{ conn = e.payload; statusKnown = true; renderCountdown(true); });
})();
