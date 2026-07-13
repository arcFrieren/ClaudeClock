/* ClaudeGraph (BETA) — módulo compartido entre ventana principal y Configuración.
   Reutiliza los nodos de barra y solo ajusta alturas (SPEC §9.3). */
"use strict";

/* Auto-ajuste: la ventana siempre mide exactamente lo que ocupa su contenido.
   Se llama tras cada cambio de layout (pestañas, bloques que aparecen, etc.). */
function fitWindow(width){
  requestAnimationFrame(()=>{
    const tb = document.querySelector(".titlebar");
    let h = tb ? tb.offsetHeight : 0;
    document.querySelectorAll(".content > *").forEach(el=>{ h += el.offsetHeight; });
    const { LogicalSize } = window.__TAURI__.dpi;
    window.__TAURI__.window.getCurrentWindow()
      .setSize(new LogicalSize(width, Math.min(h + 2, 940)));
  });
}

function createGraph(root){
  const { invoke } = window.__TAURI__.core;
  const barsEl = root.querySelector(".spark-bars");
  const tipEl  = root.querySelector(".gtip");
  const projEl = root.querySelector(".proj");
  const tabs   = root.querySelectorAll(".gtabs button");
  const COLOR  = { s:"var(--ses)", w:"var(--claude)", f:"var(--mago)" };
  const DIAS   = ["dom","lun","mar","mié","jue","vie","sáb"];
  const p2 = n => String(n).padStart(2,"0");

  let meter = "s";
  let data = null;          // { buckets, start_ts, step_secs }
  let nodes = [];
  let crFn = () => ({ on:false, total:0 });

  function label(i){
    const t0 = new Date((data.start_ts + i*data.step_secs)*1000);
    const t1 = new Date((data.start_ts + (i+1)*data.step_secs)*1000);
    if(meter === "s")
      return `${p2(t0.getHours())}:${p2(t0.getMinutes())}–${p2(t1.getHours())}:${p2(t1.getMinutes())}`;
    return `${DIAS[t0.getDay()]} ${p2(t0.getHours())}–${p2(t1.getHours())}`;
  }

  function render(){
    const vals = data.buckets;
    const max = Math.max(...vals, 1);
    // crear nodos solo si cambió la cantidad (10 sesión / 28 semanal·fable)
    if(nodes.length !== vals.length){
      barsEl.innerHTML = "";
      nodes = vals.map(()=>{
        const b = document.createElement("div");
        b.className = "b";
        barsEl.appendChild(b);
        return b;
      });
      nodes.forEach((b,i)=>{
        b.addEventListener("mousemove", ()=>{
          const v = Math.round(data.buckets[i]*10)/10;
          let t = `${label(i)} · ${v}%`;
          const cr = crFn();
          if(cr.on) t += ` · ${Math.round(v/100*cr.total).toLocaleString("es-MX")} cr`;
          tipEl.textContent = t;
          tipEl.style.display = "block";
          const r = barsEl.getBoundingClientRect(), br = b.getBoundingClientRect();
          tipEl.style.left = (br.left - r.left + br.width/2) + "px";
          tipEl.style.top  = (br.top - r.top) + "px";
        });
        b.addEventListener("mouseleave", ()=>tipEl.style.display = "none");
      });
    }
    nodes.forEach((b,i)=>{
      const v = vals[i];
      b.style.background = COLOR[meter];
      b.style.height = Math.max(3,(v/max)*100) + "%";
      b.style.opacity = v === 0 ? ".18" : "";
    });
    tabs.forEach(bt=>{ bt.className = bt.dataset.t === meter ? "on-"+meter : ""; });
  }

  function renderProj(p){
    projEl.innerHTML = p
      ? `<span class="tag">⟳ PROYECCIÓN</span> | Podría agotarse el <b>${p}</b>`
      : `<span class="tag">⟳ PROYECCIÓN</span> | <b>sin ritmo de consumo suficiente</b>`;
  }

  async function refresh(){
    data = await invoke("get_graph", { meter });
    render();
    renderProj(await invoke("get_projection", { meter }));
  }

  tabs.forEach(bt=>bt.addEventListener("click", ()=>{ meter = bt.dataset.t; refresh(); }));

  return {
    refresh,
    setCr(fn){ crFn = fn; },
    get meter(){ return meter; }
  };
}
