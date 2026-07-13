# SPEC — ClaudeClock v1.0 (consolidado)

Especificación final para construir en **Claude Code**. El diseño visual de referencia está en
dos archivos que acompañan a este documento:
- `finalistas-v2-barra-overlay.html` → los 6 temas de la barra, estados de actividad y toggle de fondo.
- `claudeclock-v3.html` → ventana ClaudeClock, ClaudeGraph y Configuración (comportamiento exacto,
  popups, colores y layout). **La maqueta es la fuente de verdad visual: replicar tal cual.**

## 1. Producto

**ClaudeClock**: app de escritorio para Windows que muestra el consumo de la suscripción Claude
del usuario en una barra overlay delgada, siempre visible, anclada sobre la barra de tareas.
Tres medidores: **Sesión** (ventana móvil 5 h, blanco), **Semanal** (7 días, naranja Claude
#D97757) y **Fable** (límite semanal del modelo superior, morado #9257E8).

Tres superficies:
1. **Barra overlay** — 6 temas seleccionables en vivo (Terminal, Geist Minimal, Data ticks,
   Condensada triple, Hyperlegible, Blueprint). Sin fondo (translúcido) es el modo predeterminado.
2. **Ventana ClaudeClock** (doble clic en la barra) — medidores grandes, créditos, ClaudeGraph.
3. **Configuración** — pestañas: ClaudeClock · ClaudeGraph (BETA) · Datos · Cuenta.

## 2. Stack

- **Tauri 2.x** (backend Rust + webview del sistema, sin Chromium embebido).
- Frontend **vanilla** HTML/CSS/JS (sin frameworks): una vista por superficie.
- Fuente **Geist Mono** empaquetada localmente en woff2, solo pesos 300 y 500. Cero CDN en runtime.
- Los temas de barra usan las fuentes de `finalistas-v2` (JetBrains Mono, DM Mono, Barlow
  Condensed, Atkinson Hyperlegible Mono, SUSE Mono), también empaquetadas.

## 3. Origen de datos

No existe API pública para consumo de suscripción claude.ai. La app reutiliza la sesión del usuario:

1. **Login**: ventana webview a `https://claude.ai/login`; el usuario inicia sesión manualmente
   (incluido 2FA). La app nunca ve ni guarda la contraseña; persiste solo la cookie de sesión en
   la partición cifrada del webview.
2. **Endpoint**: descubrir con el usuario logueado la petición interna que alimenta
   `claude.ai/settings/usage` (típicamente bajo `/api/organizations/{org_id}/...`) inspeccionando
   la red. Guardar la ruta en config para actualizarla sin recompilar.
3. **Polling**: cada 60 s (configurable 30 s–5 min). Con backoff exponencial ante errores.
4. **Detección de actividad**: comparar % entre polls. Si Sesión/Semanal subieron → estado
   "actividad" (glow + pulso en la barra). Fable solo se enciende si su propio % subió.
5. **Fallback**: 401 o cambio de endpoint → estado "reconectar" en la barra + botón de re-login.

Advertencia visible en Cuenta (ya redactada en la maqueta v3… fue eliminada del layout final,
mostrarla solo en el instalador/README): endpoint interno no documentado, solo lectura, solo
la cuenta del usuario, datos únicamente locales.

## 4. Barra overlay

- Sin marco, `always_on_top`, sin Alt-Tab ni taskbar. Anclada al borde superior del área de
  trabajo de la taskbar del monitor configurado.
- Alto: ½ del alto real de la taskbar (leerlo por API de Windows).
- **Alto de barra** (4 modos): Fino izquierda (default) · Compacto izquierda · Fino derecha ·
  Compacto derecha. Fino = ancho completo del monitor; Compacto = ~45% anclado al lado indicado.
- Estados por medidor: reposo = opacidad 32%; actividad = opacidad 100% + glow + pulso de 1 s
  en cada actualización con actividad. Umbrales: ≥80% color advertencia, ≥90% alerta +
  notificación nativa una vez por cruce.
- Interacción: doble clic → ventana ClaudeClock; clic derecho → menú (monitor, clic-through,
  pausar, salir). Auto-ocultar cuando una app está en pantalla completa en ese monitor.
- Temas Hyperlegible y Blueprint: seleccionables solo en modo Con fondo. En modo Sin fondo
  aparecen al 35% de opacidad y un tooltip que sigue al cursor (abajo-derecha) muestra
  "Exclusivo Con Fondo" en recuadro de borde fino y texto naranja Claude.

## 5. Ventana ClaudeClock

Replicar `claudeclock-v3.html`:
- Cabecera: spark de Claude animado (SVG, rotación+escala por transform) + "ClaudeClock" +
  "sinc. hace N s" (todo al mismo tamaño/color gris tenue) + interruptor **cr** + botón
  ⚙ Configuración, pegados a la derecha.
- Medidores: nombre + LED de actividad, %, barra de color, y al pie: créditos
  "usados / [total editable inline] · quedan X" (solo si cr está activo) y el contador de
  reinicio siempre a la derecha (sin hora absoluta).
- **cr**: al activarse por primera vez, popup ATENCIÓN (créditos relativos, Anthropic no
  publica totales) con "No volver a mostrar" marcado + botón Entendido. Los 3 totales son
  independientes y editables (confirmado: no derivables entre sí).
- Bloque ClaudeGraph: visible solo si Activar ClaudeGraph + Mostrar en ventana principal.

## 6. ClaudeGraph (BETA)

- Tabs Sesión / Semanal / Fable. Sesión: 10 barras (30 min c/u, 5 h). Semanal y Fable:
  28 barras (6 h c/u, 7 días). Colores blanco / naranja / morado; barras en 0 casi invisibles.
- Hover: tooltip con intervalo + % (+ créditos si cr activo, calculados con el total del medidor).
- Proyección: "⟳ PROYECCIÓN | Podría agotarse el <día>-<hh:mm>" — "⟳ PROYECCIÓN" en ámbar,
  el resto gris tenue; siempre nombre de día, nunca "hoy". Cálculo: regresión simple del ritmo
  de las últimas N horas contra el % restante.
- Título "CLAUDEGRAPH" en naranja Claude, "(BETA)" en ámbar.
- En Configuración la gráfica es alta (~180 px); en ClaudeClock, compacta (~67 px).
- Al activar por primera vez: popup ATENCIÓN (función no finalizada) Sí/No + "No volver a mostrar".
- "Mostrar en ventana principal" bloqueado/atenuado mientras ClaudeGraph esté desactivado.

## 7. Configuración

- **ClaudeClock**: Personalización (grid de 6 temas con miniaturas), y fila de 3 menús:
  Fondo de barra (Sin fondo default · Con fondo) · Alto de barra (4 opciones) ·
  Monitor de Barra (Monitor 1 default · Monitor 2 · Seguir al cursor). Títulos en blanco.
  Si el usuario pasa a Sin fondo con un tema exclusivo seleccionado → volver a Geist.
- **ClaudeGraph (BETA)**: los dos interruptores + la gráfica expandida.
- **Datos**: Guardar historial local (on default, "Recomendado para ClaudeGraph."),
  Intervalo de actualización ("Cada cuánto consulta tu uso."), y botón rojo
  "Borrar historial guardado" → popup ATENCIÓN irreversible Sí/No.
- **Cuenta**: estado de sesión + botón verde "Volver a iniciar sesión";
  "Iniciar con Windows" desactivado por defecto.

Config persistente (JSON local):
`{ tema, fondo, alto_modo, monitor, intervalo, cr, cr_avisado, graph_on, graph_avisado,
graph_en_principal, totales:{s,w,f}, historial_on, autoarranque, click_through, endpoint }`

## 8. Historial

Guardar desde el primer arranque aunque ClaudeGraph esté apagado (decisión: "preparado,
activar después"). Formato: JSONL append-only `{ts, s, w, f}` por poll, con **buffer en RAM
volcado a disco cada 5 min** y compactación diaria a resolución de 30 min tras 7 días.
Sin base de datos: en 1 año son unos pocos MB.

## 9. Rendimiento — objetivo: correr en equipos básicos

Requisitos mínimos objetivo: Windows 10, CPU dual-core, 2 GB RAM, gráficos integrados viejos.
Presupuesto: **RAM < 40 MB, CPU ~0% en reposo y < 1% durante el tick**, instalador < 10 MB.

Reglas obligatorias de implementación:
1. Animar **solo `transform` y `opacity`** (composición en GPU). Nada de `filter`,
   `box-shadow` ni `drop-shadow` animados en bucle.
2. **Modo bajo consumo** (toggle en Datos + autodetección de hardware lento): desactiva el
   glow, el spark animado y todas las transiciones; el pulso de actividad se reduce a un
   cambio de opacidad instantáneo. `prefers-reduced-motion` lo activa también.
3. **Un solo timer** de 1 s para las cuentas regresivas; actualizar `textContent` únicamente
   si el valor cambió; ninguna reconstrucción de DOM — el gráfico reutiliza sus nodos y solo
   ajusta alturas.
4. **Pausar todo render** cuando la barra está auto-oculta (fullscreen), el monitor apagado,
   o la ventana ClaudeClock cerrada (visibilitychange + eventos de Tauri). El polling de red
   continúa (para no perder historial) pero sin tocar el DOM.
5. Red: solo el poll cada 60 s; sin websockets, sin peticiones paralelas; backoff ante fallos.
6. I/O: historial con buffer (regla §8) para no castigar discos duros lentos.
7. Cero dependencias JS de runtime; fuentes woff2 locales con `font-display:swap`.
8. El cálculo de proyección corre en el backend Rust, no en el webview.

## 10. Entregables y aceptación

1. Proyecto Tauri compilable (`cargo tauri build`) con instalador NSIS/MSI y flag `--demo`
   (datos simulados, los mismos de las maquetas).
2. README: primer arranque, login, re-descubrimiento de endpoint, desinstalación.
3. Criterios:
   - [ ] Sobrevive reinicios sin pedir login mientras la sesión viva.
   - [ ] Actualiza los 3 medidores sin interacción, ≤ 60 s de retraso.
   - [ ] Nunca roba el foco; no aparece en Alt-Tab.
   - [ ] Cumple el presupuesto de RAM/CPU en un equipo de gama baja (§9).
   - [ ] Multi-monitor: recuerda su monitor y sobrevive desconexión/reconexión.
   - [ ] Los 3 popups ATENCIÓN respetan "No volver a mostrar" entre reinicios.
   - [ ] Paridad visual con las dos maquetas HTML.
