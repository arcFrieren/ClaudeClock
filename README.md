# ClaudeClock

Barra overlay para Windows que muestra en vivo el consumo de tu suscripción de
Claude: **Sesión** (ventana de 5 h), **Semanal** (7 días) y **Fable** (límite
semanal del modelo superior). Ligera (~35 MB de RAM), siempre visible, con
icono en la bandeja del sistema.

## Instalación

1. Descarga `ClaudeClock_x64-setup.exe` desde
   [Releases](../../releases) y ejecútalo.
2. Listo. La barra aparece sobre la pantalla y el icono queda en la bandeja
   del sistema (iconos ocultos). Arranca solo con Windows.

**Requisitos:** Windows 10/11. Nada más.

## Conexión con tu cuenta

- Si usas **Claude Code** y tienes sesión iniciada, ClaudeClock se conecta solo
  usando tu token local. No hay que hacer nada.
- Si no, se abre una ventana para iniciar sesión en claude.ai (usuario, 2FA…).
  La app nunca ve tu contraseña y todos los datos se quedan en tu equipo.

## Uso

| Acción | Resultado |
|---|---|
| Clic izquierdo en el icono de la bandeja | Abre el dashboard |
| Clic derecho en el icono | Menú: monitor, clic-through, pausar, salir |
| ⚙ Configuración | Temas de barra, posición, tamaño, ClaudeGraph, intervalo |
| Minimizar / cerrar ventanas | Se ocultan a la bandeja; la barra sigue |

Modo demostración (datos simulados): `ClaudeClock.exe --demo`

## Compilar desde el código

Requiere [Rust](https://rustup.rs) (MSVC), VS Build Tools 2022 con C++ y Node.js.

```powershell
npm install
npm run demo    # correr en modo demo
npm run build   # generar el instalador (src-tauri/target/release/bundle/nsis/)
```

## Desinstalar

Configuración de Windows → Aplicaciones → ClaudeClock. Para borrar también la
configuración e historial: elimina `%APPDATA%\com.claudeclock.app\`.

---

Los documentos de diseño (SPEC y maquetas HTML) están en [`docs/`](docs/).
