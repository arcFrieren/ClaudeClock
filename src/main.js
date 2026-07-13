/* Ventana ClaudeClock */
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

const METERS = ["s","w","f"];
let cfg = null;
let usage = null;
let graph = null;

const $ = s => document.querySelector(s);
const p2 = n => String(n).padStart(2,"0");

/* ---------- config ---------- */
function saveCfg(){ invoke("set_config", { cfg }); }
let totTimer = null;

function applyConfig(c){
  cfg = c;
  document.body.classList.toggle("no-cr", !c.cr);
  $("#swCr").classList.toggle("on", c.cr);
  METERS.forEach(m=>{
    const inp = document.querySelector(`.meter[data-m="${m}"] .tot`);
    if(document.activeElement !== inp) inp.value = c.totales[m];
  });
  const vis = c.graph_on && c.graph_en_principal;
  $("#mainGraph").style.display = vis ? "block" : "none";
  if(vis) graph.refresh().then(()=>fitWindow(560));
  refreshCredits();
  fitWindow(560);
}

/* ---------- medidores ---------- */
function refreshCredits(){
  if(!usage || !cfg) return;
  METERS.forEach(m=>{
    const mt = document.querySelector(`.meter[data-m="${m}"]`);
    const tot = cfg.totales[m] || 0;
    const used = Math.round(usage[m]/100*tot);
    mt.querySelector(".used").textContent = used.toLocaleString("es-MX");
    mt.querySelector(".left").textContent = (tot-used).toLocaleString("es-MX");
  });
}

function render(u){
  usage = u;
  METERS.forEach(m=>{
    const mt = document.querySelector(`.meter[data-m="${m}"]`);
    const pct = Math.round(u[m]);
    const el = mt.querySelector(".pct");
    if(el.textContent !== pct+"%"){
      el.textContent = pct+"%";
      mt.querySelector(".fill").style.width = pct+"%";
    }
  });
  refreshCredits();
}

/* ---------- timer único de 1 s: sync + cuentas regresivas ---------- */
const agoEl = $(".sync .ago");
const cache = { ago:"", s:"", w:"" };
function tick(){
  if(!usage) return;
  const now = Math.floor(Date.now()/1000);
  const ago = `hace ${Math.max(0, now-usage.ts)} s`;
  if(ago !== cache.ago){ cache.ago = ago; agoEl.textContent = ago; }

  const ls = Math.max(0, usage.reset_s - now);
  const ts = `Reinicio en ${p2(Math.floor(ls/3600))}:${p2(Math.floor(ls%3600/60))}:${p2(ls%60)}`;
  if(ts !== cache.s){ cache.s = ts; document.querySelector('.meter[data-m="s"] .reset').textContent = ts; }

  const lw = Math.max(0, usage.reset_w - now);
  const tw = `Reinicio en ${Math.floor(lw/86400)} d ${Math.floor(lw%86400/3600)} h`;
  if(tw !== cache.w){
    cache.w = tw;
    document.querySelector('.meter[data-m="w"] .reset').textContent = tw;
    document.querySelector('.meter[data-m="f"] .reset').textContent = tw;
  }
}
let timer = setInterval(tick, 1000);
document.addEventListener("visibilitychange", ()=>{
  clearInterval(timer); timer = null;
  if(!document.hidden){
    if(usage) render(usage);
    tick();
    timer = setInterval(tick, 1000);
  }
});

/* ---------- interruptor cr + popup ATENCIÓN ---------- */
$("#swCr").addEventListener("click", ()=>{
  if(!cfg.cr && !cfg.cr_avisado){ $("#ovCr").classList.add("on"); return; }
  cfg.cr = !cfg.cr; saveCfg();
});
$("#crOk").addEventListener("click", ()=>{
  if($("#crNoShow").checked) cfg.cr_avisado = true;
  $("#ovCr").classList.remove("on");
  cfg.cr = true; saveCfg();
});

/* ---------- totales editables inline ---------- */
document.querySelectorAll(".meter .tot").forEach(inp=>{
  inp.addEventListener("input", ()=>{
    const m = inp.closest(".meter").dataset.m;
    cfg.totales[m] = parseInt(inp.value,10) || 0;
    refreshCredits();
    clearTimeout(totTimer);
    totTimer = setTimeout(saveCfg, 600);
  });
});

/* ---------- botones ---------- */
$("#btnCfg").addEventListener("click", ()=>invoke("open_settings"));
$("#btnClose").addEventListener("click", ()=>getCurrentWindow().close());
$("#btnMin").addEventListener("click", ()=>getCurrentWindow().hide()); // a la bandeja

/* ---------- arranque ---------- */
(async ()=>{
  graph = createGraph($("#mainGraph"));
  graph.setCr(()=>({ on: cfg?.cr, total: cfg?.totales[graph.meter] || 0 }));
  const boot = await invoke("boot");
  applyConfig(boot.config);
  if(boot.usage){ render(boot.usage); tick(); }
  await listen("usage", e=>{
    if(document.hidden){ usage = e.payload; return; }  // sin tocar DOM oculto (SPEC §9.4)
    render(e.payload);
    if(cfg.graph_on && cfg.graph_en_principal) graph.refresh();
  });
  await listen("config-changed", e=>applyConfig(e.payload));
})();
