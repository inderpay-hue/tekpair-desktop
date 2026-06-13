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

## Empaquetar la web dentro (offline total) — opcional

Por defecto carga la web remota (autoactualizable). Si quisieras meter el HTML/JS
dentro del instalador (offline total, pero hay que reconstruir en cada cambio),
copia el frontend de TekPair a `dist/` y cambia en `src-tauri/tauri.conf.json` el
`app.windows[0].url` a `index.html`.
