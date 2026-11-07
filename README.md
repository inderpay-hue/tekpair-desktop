# TekPair Desktop

App de escritorio (Tauri 2) que envuelve **TekPair** en una ventana nativa para
Windows y macOS, con **impresión nativa** a la impresora del sistema.

Carga la web en producción (`https://www.tekpair.tech`), así que **se autoactualiza**:
cualquier despliegue de TekPair llega sin reinstalar nada. El Service Worker de
TekPair ya da caché offline del armazón + datos locales. La sincronización entre
equipos es instantánea (Supabase Realtime, ya integrado en la web).

## Cómo compilar (sin tener Windows ni Mac)

El workflow `.github/workflows/build.yml` compila los instaladores en GitHub Actions:

1. Crea un repo en GitHub (p. ej. `inderpay-hue/tekpair-desktop`) y sube esta carpeta.
2. Cada `push` a `main` genera los instaladores como *artifacts* (Actions → último run).
3. Para publicar una **Release** con los instaladores, crea un tag:
   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```
   - Windows → `.exe` (NSIS)
   - macOS → `.dmg`

## Firma de código (recomendado, opcional)

- **Windows:** sin firma, SmartScreen avisa la primera vez ("Más información →
  Ejecutar de todas formas"). Para quitarlo hace falta un certificado EV/OV.
- **macOS:** sin firma/notarización, Gatekeeper bloquea ("desarrollador no
  identificado"; abrir con clic derecho → Abrir). Para quitarlo hace falta cuenta
  de Apple Developer (99 $/año) y configurar `APPLE_CERTIFICATE`, `APPLE_ID`, etc.
  en los secrets del repo.

## Impresión nativa

La web detecta que corre dentro de Tauri (`window.__TAURI__`) y puede llamar a:

- `list_printers()` → lista de impresoras del sistema.
- `print_raw(printer, bytes)` → envía bytes crudos (ESC/POS, ZPL, EPL…) a esa
  impresora, en silencio, usando el driver ya instalado.
  - Windows: API `winspool` (RAW).
  - macOS/Linux: CUPS (`lp -o raw`).

> **Pendiente (fase 2):** las etiquetas de TekPair hoy se imprimen como HTML con el
> diálogo del navegador. La impresión HTML **silenciosa** (render → PDF → impresora)
> se añadirá como comando `print_pdf()`.

## TPV en una ventana aparte (v0.3.0)

Al pulsar «TPV» en el panel, la web (`js/tpv-ventana.js` de TekPair) llama a
`abrir_ventana({ cual: 'tpv' })`: se abre la ventana nativa `tpv`
(`https://www.tekpair.tech/tpv.html`, 1280×840) y el panel se queda en la principal.
Si ya está abierta no se abre otra: se des-minimiza, se muestra y se enfoca. Si el
usuario la cerró, la siguiente vez se crea de nuevo. Desde el TPV, «Reparación»,
«Cliente», «Stock», «Pedido» y «Dashboard» llevan la acción a la ventana principal
(`abrir_ventana({ cual: 'panel', ruta? })`). Solo se aceptan esas dos ventanas y
rutas `/tpv.html…` o `/dashboard.html…`.

- Permiso `ventanas` (src-tauri/permissions/impresion.toml) y la capability `default`
  cubre las ventanas `main` y `tpv` (las dos pueden imprimir). La ventana la crea Rust,
  así que no hace falta `core:webview:allow-create-webview-window` para el JS.
- `tauri-plugin-window-state` recuerda tamaño y posición de cada ventana.
- Con una app instalada < 0.3.0 la orden no existe: la web lo detecta (la llamada
  falla) y abre el TPV en la misma ventana, como siempre.

## Publicar una versión nueva (con autoactualización)

Las apps ≥ 0.2.0 miran `releases/latest/download/latest.json` al abrirse y preguntan
antes de instalar. Pasos (desde la rama lista, p. ej. `feat/tpv-ventana`):

1. Versión subida en los tres sitios: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
   y `package.json` (ya está en 0.3.0 en `feat/tpv-ventana`).
2. Fusionar en `main` y `git push origin main` → GitHub Actions compila el instalador
   (artifact `tekpair-windows-installer`). Comprobar que el build pasa (es la primera
   compilación con `abrir_ventana` y `tauri-plugin-window-state`).
3. Probar el `.exe` del artifact en un Windows: TPV abre ventana aparte, pulsar otra vez
   la trae delante (también minimizada), cerrarla y volver a abrirla, imprimir desde la
   ventana del TPV, «Reparación» desde el TPV lleva al panel.
4. Publicar: `git tag v0.3.0 && git push origin v0.3.0` → la Release sube
   `TekPair-Setup.exe`, `.sig` y `latest.json` (firmados con los secretos
   `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)` del repo). El enlace de descarga de la web
   (`releases/latest/download/TekPair-Setup.exe`) no cambia.
5. Cuenta de GitHub correcta: `inderpay-hue` (no `indertatla24-creator`).

## Empaquetar la web dentro (offline total) — opcional

Por defecto carga la web remota (autoactualizable). Si quisieras meter el HTML/JS
dentro del instalador (offline total, pero hay que reconstruir en cada cambio),
copia el frontend de TekPair a `dist/` y cambia en `src-tauri/tauri.conf.json` el
`app.windows[0].url` a `index.html`.
