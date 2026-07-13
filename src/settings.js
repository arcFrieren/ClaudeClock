/* Ventana Configuración */
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

let cfg = null;
let graph = null;
const $ = s => document.querySelector(s);
const $$ = s => document.querySelectorAll(s);

function saveCfg(){ invoke("set_config", { cfg }); }

/* ---------- aplicar config al UI ---------- */
function applyConfig(c){
  cfg = c;

  // temas: selección + bloqueo de exclusivos en Sin fondo
  const sin = c.fondo === "sin";
  $$(".theme").forEach(t=>{
    t.classList.toggle("sel", t.dataset.th === c.tema);
    if(t.hasAttribute("data-needsbg")) t.classList.toggle("locked", sin);
  });
  $$("#fondoCtrl button").forEach(b=>b.classList.toggle("on", b.dataset.f === c.fondo));
  $("#selAlto").value = c.alto_modo;
  $("#selPos").value = c.posicion;
  if($("#selMon").options.length) $("#selMon").value = c.monitor;

  // ClaudeGraph
  $("#swGraph").classList.toggle("on", c.graph_on);
  $("#swShowMain").classList.toggle("on", c.graph_en_principal);
  $("#swShowMain").classList.toggle("disabled", !c.graph_on);
  $("#tgShowMain").classList.toggle("dim", !c.graph_on);
  $("#cfgGraph").style.opacity = c.graph_on ? "1" : ".35";
  if(c.graph_on) graph.refresh().then(()=>fitWindow(660));

  // Datos
  $("#swHist").classList.toggle("on", c.historial_on);
  $("#selInt").value = String(c.intervalo);

  // Cuenta
  $("#swAuto").classList.toggle("on", c.autoarranque);

  fitWindow(660);
}

/* ---------- navegación de pestañas ---------- */
$$(".cfg-nav button").forEach(btn=>{
  btn.addEventListener("click", ()=>{
    $$(".cfg-nav button").forEach(b=>b.classList.remove("on"));
    $$(".cfg-pane").forEach(p=>p.classList.remove("on"));
    btn.classList.add("on");
    document.querySelector(`.cfg-pane[data-pane="${btn.dataset.pane}"]`).classList.add("on");
    if(btn.dataset.pane === "graph" && cfg.graph_on) graph.refresh().then(()=>fitWindow(660));
    fitWindow(660);
  });
});

/* ---------- temas ---------- */
$$(".theme").forEach(t=>{
  t.addEventListener("click", ()=>{
    if(t.classList.contains("locked")) return;
    cfg.tema = t.dataset.th;
    applyConfig(cfg); saveCfg();
  });
});

/* tooltip que sigue al cursor en temas bloqueados */
const lockTip = $("#lockTip");
$$(".theme[data-needsbg]").forEach(t=>{
  t.addEventListener("mousemove", e=>{
    if(!t.classList.contains("locked")){ lockTip.style.display = "none"; return; }
    lockTip.style.display = "block";
    lockTip.style.left = (e.clientX+14) + "px";
    lockTip.style.top  = (e.clientY+18) + "px";
  });
  t.addEventListener("mouseleave", ()=>lockTip.style.display = "none");
});

/* ---------- fondo de barra ---------- */
$$("#fondoCtrl button").forEach(bt=>{
  bt.addEventListener("click", ()=>{
    cfg.fondo = bt.dataset.f;
    // al pasar a Sin fondo con un tema exclusivo seleccionado → volver a Geist (SPEC §7)
    if(cfg.fondo === "sin" && (cfg.tema === "hyper" || cfg.tema === "blue")) cfg.tema = "geist";
    applyConfig(cfg); saveCfg();
  });
});

/* ---------- posición / tamaño / monitor ---------- */
$("#selPos").addEventListener("change", e=>{ cfg.posicion = e.target.value; saveCfg(); });
$("#selAlto").addEventListener("change", e=>{ cfg.alto_modo = e.target.value; saveCfg(); });
$("#selMon").addEventListener("change", e=>{ cfg.monitor = e.target.value; saveCfg(); });

/* ---------- ClaudeGraph on/off + mostrar en principal ---------- */
$("#swGraph").addEventListener("click", ()=>{
  if(!cfg.graph_on && !cfg.graph_avisado){ $("#ovGraph").classList.add("on"); return; }
  cfg.graph_on = !cfg.graph_on;
  if(!cfg.graph_on) cfg.graph_en_principal = false;
  applyConfig(cfg); saveCfg();
});
$("#grSi").addEventListener("click", ()=>{
  if($("#grNoShow").checked) cfg.graph_avisado = true;
  $("#ovGraph").classList.remove("on");
  cfg.graph_on = true;
  applyConfig(cfg); saveCfg();
});
$("#grNo").addEventListener("click", ()=>$("#ovGraph").classList.remove("on"));
$("#swShowMain").addEventListener("click", ()=>{
  if(!cfg.graph_on) return;
  cfg.graph_en_principal = !cfg.graph_en_principal;
  applyConfig(cfg); saveCfg();
});

/* ---------- Datos ---------- */
$("#swHist").addEventListener("click", ()=>{ cfg.historial_on = !cfg.historial_on; applyConfig(cfg); saveCfg(); });
$("#selInt").addEventListener("change", e=>{ cfg.intervalo = parseInt(e.target.value,10); saveCfg(); });
$("#btnDel").addEventListener("click", ()=>$("#ovDel").classList.add("on"));
$("#delSi").addEventListener("click", async ()=>{
  await invoke("clear_history");
  $("#ovDel").classList.remove("on");
});
$("#delNo").addEventListener("click", ()=>$("#ovDel").classList.remove("on"));

/* ---------- Cuenta ---------- */
$("#btnLogin").addEventListener("click", ()=>invoke("relogin"));
$("#swAuto").addEventListener("click", ()=>{ cfg.autoarranque = !cfg.autoarranque; applyConfig(cfg); saveCfg(); });

$("#btnClose").addEventListener("click", ()=>getCurrentWindow().close());
$("#btnMin").addEventListener("click", ()=>getCurrentWindow().hide()); // a la bandeja

/* ---------- arranque ---------- */
(async ()=>{
  graph = createGraph($("#cfgGraph"));
  graph.setCr(()=>({ on: cfg?.cr, total: 0 }));
  const boot = await invoke("boot");

  // monitores disponibles + Seguir al cursor
  const sel = $("#selMon");
  boot.monitors.forEach((name,i)=>{
    const o = document.createElement("option");
    o.value = String(i+1);
    o.textContent = `Monitor ${i+1}`;
    sel.appendChild(o);
  });
  const oc = document.createElement("option");
  oc.value = "cursor"; oc.textContent = "Seguir al cursor";
  sel.appendChild(oc);

  // estado de cuenta (se actualiza en vivo con los eventos "status")
  function setAccount(connected, demo, source){
    if(demo){
      $("#stTxt").textContent = "Modo demo — datos simulados";
      $("#stDot").classList.add("off");
    } else if(connected){
      $("#stTxt").textContent = source === "token"
        ? "Sesión activa vía Claude Code (token local)"
        : "Sesión de claude.ai activa";
      $("#stDot").classList.remove("off");
    } else {
      $("#stTxt").textContent = "Sin sesión — inicia sesión en Claude Code o aquí";
      $("#stDot").classList.add("off");
    }
  }
  setAccount(boot.connected, boot.demo, boot.source);

  applyConfig(boot.config);
  await listen("config-changed", e=>applyConfig(e.payload));
  await listen("status", e=>setAccount(e.payload.connected, e.payload.demo, e.payload.source));
})();
