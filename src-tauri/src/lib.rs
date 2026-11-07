// TekPair Desktop — carcasa nativa que carga https://www.tekpair.tech y expone
// impresión NATIVA a la impresora del sistema, cross-platform.
//
// La web detecta que corre dentro de Tauri (window.__TAURI__) y llama a:
//   - list_printers() -> ["EPSON TM-T20", "POS-80", "Brother QL-800", ...]
//   - print_raw(printer, data) -> envía esos bytes tal cual a la impresora (ESC/POS,
//     ZPL, EPL... lenguaje de etiquetas/tickets). Silencioso, usa el driver del sistema.
//   - print_label(printer, png, w_mm, h_mm, copies) -> imprime una etiqueta como
//     imagen en silencio (la web la renderiza a PNG con html2canvas).
//   - abrir_ventana(cual, ruta?) -> abre o trae delante la ventana del TPV o la del panel.
//
// Windows: winspool (RAW) + `mspaint /pt` (imagen). macOS/Linux: CUPS (`lp`, `lpstat`).

// Escribe bytes a un archivo temporal único y devuelve su ruta. Sin dependencias de
// aleatoriedad: usa el PID + un contador atómico para el nombre.
use std::sync::atomic::{AtomicU64, Ordering};
static _TMP_SEQ: AtomicU64 = AtomicU64::new(0);
pub(crate) fn write_temp(prefix: &str, ext: &str, data: &[u8]) -> Result<std::path::PathBuf, String> {
    let n = _TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("{}_{}_{}.{}", prefix, std::process::id(), n, ext));
    std::fs::write(&path, data).map_err(|e| format!("No se pudo escribir temporal: {}", e))?;
    Ok(path)
}

// ───────────────────────── Windows: impresión RAW vía winspool ─────────────────────────
#[cfg(windows)]
mod printing {
    use std::ffi::c_void;
    use std::ptr;

    type HANDLE = *mut c_void;
    type BOOL = i32;
    type DWORD = u32;
    type LPWSTR = *mut u16;

    #[repr(C)]
    struct DocInfo1 {
        p_doc_name: LPWSTR,
        p_output_file: LPWSTR,
        p_datatype: LPWSTR,
    }

    #[repr(C)]
    struct PrinterInfo4 {
        p_printer_name: LPWSTR,
        p_server_name: LPWSTR,
        attributes: DWORD,
    }

    #[link(name = "winspool")]
    extern "system" {
        fn OpenPrinterW(p_printer_name: LPWSTR, ph_printer: *mut HANDLE, p_default: *mut c_void) -> BOOL;
        fn StartDocPrinterW(h_printer: HANDLE, level: DWORD, p_doc_info: *mut DocInfo1) -> DWORD;
        fn StartPagePrinter(h_printer: HANDLE) -> BOOL;
        fn WritePrinter(h_printer: HANDLE, p_buf: *const c_void, cb_buf: DWORD, pc_written: *mut DWORD) -> BOOL;
        fn EndPagePrinter(h_printer: HANDLE) -> BOOL;
        fn EndDocPrinter(h_printer: HANDLE) -> BOOL;
        fn ClosePrinter(h_printer: HANDLE) -> BOOL;
        fn EnumPrintersW(flags: DWORD, name: LPWSTR, level: DWORD, p_printer_enum: *mut u8, cb_buf: DWORD, pcb_needed: *mut DWORD, pc_returned: *mut DWORD) -> BOOL;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn read_wide(p: LPWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }

    pub fn print_raw(printer: &str, data: &[u8]) -> Result<(), String> {
        unsafe {
            let mut name = to_wide(printer);
            let mut h: HANDLE = ptr::null_mut();
            if OpenPrinterW(name.as_mut_ptr(), &mut h, ptr::null_mut()) == 0 {
                return Err(format!("No se pudo abrir la impresora '{}'", printer));
            }
            let mut doc_name = to_wide("TekPair");
            let mut datatype = to_wide("RAW");
            let mut di = DocInfo1 {
                p_doc_name: doc_name.as_mut_ptr(),
                p_output_file: ptr::null_mut(),
                p_datatype: datatype.as_mut_ptr(),
            };
            if StartDocPrinterW(h, 1, &mut di) == 0 {
                ClosePrinter(h);
                return Err("StartDocPrinter falló".into());
            }
            if StartPagePrinter(h) == 0 {
                EndDocPrinter(h);
                ClosePrinter(h);
                return Err("StartPagePrinter falló".into());
            }
            let mut written: DWORD = 0;
            let ok = WritePrinter(h, data.as_ptr() as *const c_void, data.len() as DWORD, &mut written);
            EndPagePrinter(h);
            EndDocPrinter(h);
            ClosePrinter(h);
            if ok == 0 {
                return Err("WritePrinter falló".into());
            }
            Ok(())
        }
    }

    // Imprime una imagen PNG (una etiqueta) en silencio. Usa `mspaint /pt`, que envía
    // la imagen a la impresora indicada sin diálogo, usando el tamaño de papel que
    // tenga configurado el driver de esa impresora (en etiquetadoras = la etiqueta).
    // width_mm/height_mm se ignoran aquí (los aplica el driver); se respetan en macOS.
    pub fn print_label(printer: &str, png: &[u8], _width_mm: f64, _height_mm: f64, copies: u32) -> Result<(), String> {
        use std::process::Command;
        let path = super::write_temp("tklabel", "png", png)?;
        let mut last_err = String::new();
        for _ in 0..copies.max(1) {
            let st = Command::new("mspaint")
                .args(["/pt", &path.to_string_lossy(), printer])
                .status();
            match st {
                Ok(s) if s.success() => {}
                Ok(s) => last_err = format!("mspaint salió con código {:?}", s.code()),
                Err(e) => last_err = format!("No se pudo lanzar mspaint: {}", e),
            }
        }
        let _ = std::fs::remove_file(&path);
        if last_err.is_empty() { Ok(()) } else { Err(last_err) }
    }

    pub fn list_printers() -> Vec<String> {
        unsafe {
            let flags: DWORD = 0x2 | 0x4; // PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS
            let level: DWORD = 4; // PRINTER_INFO_4W
            let mut needed: DWORD = 0;
            let mut returned: DWORD = 0;
            EnumPrintersW(flags, ptr::null_mut(), level, ptr::null_mut(), 0, &mut needed, &mut returned);
            if needed == 0 {
                return vec![];
            }
            let mut buf: Vec<u8> = vec![0u8; needed as usize];
            if EnumPrintersW(flags, ptr::null_mut(), level, buf.as_mut_ptr(), needed, &mut needed, &mut returned) == 0 {
                return vec![];
            }
            let stride = std::mem::size_of::<PrinterInfo4>();
            let mut out = Vec::new();
            for i in 0..returned as usize {
                let p = buf.as_ptr().add(i * stride) as *const PrinterInfo4;
                let name = read_wide((*p).p_printer_name);
                if !name.is_empty() {
                    out.push(name);
                }
            }
            out
        }
    }
}

// ───────────────────────── macOS / Linux: impresión vía CUPS (lp / lpstat) ─────────────────────────
#[cfg(not(windows))]
mod printing {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use super::write_temp;

    // Imprime una imagen PNG (una etiqueta) en silencio a tamaño exacto en mm vía CUPS.
    pub fn print_label(printer: &str, png: &[u8], width_mm: f64, height_mm: f64, copies: u32) -> Result<(), String> {
        let path = write_temp("tklabel", "png", png)?;
        let media = format!("Custom.{:.1}x{:.1}mm", width_mm, height_mm);
        let n = copies.max(1).to_string();
        let out = Command::new("lp")
            .args(["-d", printer, "-n", &n, "-o", &format!("media={}", media), "-o", "fit-to-page", path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("No se pudo lanzar lp: {}", e))?;
        let _ = std::fs::remove_file(&path);
        if !out.status.success() {
            return Err(format!("lp devolvió error: {}", String::from_utf8_lossy(&out.stderr)));
        }
        Ok(())
    }

    pub fn print_raw(printer: &str, data: &[u8]) -> Result<(), String> {
        // `lp -d <printer> -o raw` envía los bytes sin procesar (ESC/POS, ZPL...).
        let mut child = Command::new("lp")
            .args(["-d", printer, "-o", "raw"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("No se pudo lanzar lp: {}", e))?;
        {
            let stdin = child.stdin.as_mut().ok_or("Sin stdin para lp")?;
            stdin.write_all(data).map_err(|e| format!("Error escribiendo a lp: {}", e))?;
        }
        let out = child.wait_with_output().map_err(|e| format!("lp falló: {}", e))?;
        if !out.status.success() {
            return Err(format!("lp devolvió error: {}", String::from_utf8_lossy(&out.stderr)));
        }
        Ok(())
    }

    pub fn list_printers() -> Vec<String> {
        // `lpstat -a` lista colas aceptando trabajos: "EPSON_TM_T20 accepting requests..."
        let out = match Command::new("lpstat").arg("-a").output() {
            Ok(o) => o,
            Err(_) => return vec![],
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
            .collect()
    }
}

// ───────────────────────── Comandos expuestos a la web ─────────────────────────
#[tauri::command]
fn list_printers() -> Vec<String> {
    printing::list_printers()
}

#[tauri::command]
fn print_raw(printer: String, data: Vec<u8>) -> Result<(), String> {
    printing::print_raw(&printer, &data)
}

#[tauri::command]
fn print_label(printer: String, data: Vec<u8>, width_mm: f64, height_mm: f64, copies: u32) -> Result<(), String> {
    printing::print_label(&printer, &data, width_mm, height_mm, copies)
}

// ───────────────────────── Enlaces de fuera (WhatsApp, webs, email) ─────────────────────────
// La ventana carga la web de TekPair; un window.open a wa.me no abre nada dentro de la
// app. La web llama a este comando y el enlace se abre en el navegador / WhatsApp del
// sistema. Solo esquemas conocidos: nada de file:// ni rutas locales.
#[tauri::command]
fn abrir_externo(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let u = url.trim().to_string();
    let l = u.to_lowercase();
    let permitido = ["https://", "http://", "mailto:", "tel:", "whatsapp:"].iter().any(|p| l.starts_with(p));
    if !permitido {
        return Err("Enlace no permitido".into());
    }
    app.opener().open_url(u, None::<&str>).map_err(|e| e.to_string())
}

// ───────────────────────── Ventana aparte para el TPV ─────────────────────────
// En el navegador, «TPV» abre el TPV en una ventana aparte (window.open con nombre fijo)
// y el panel se queda donde estaba, para seguir con las reparaciones sin salir del TPV.
// Aquí window.open no abre ventanas (desktop-print.js lo convierte en un visor), así que
// la web llama a abrir_ventana:
//   - cual = "tpv"   → ventana con etiqueta "tpv" (https://www.tekpair.tech/tpv.html).
//   - cual = "panel" → la ventana principal ("main"), la del panel.
// Si la ventana ya existe NO se abre otra: se trae delante aunque esté minimizada o detrás
// de otras (unminimize + show + set_focus). Si el usuario la cerró, se crea de nuevo.
// `ruta` (opcional) solo puede ser una pantalla de TekPair de esa ventana (tpv.html o
// dashboard.html con su ?/#), nunca una URL cualquiera que llegue de la web.
// Tamaño y posición de cada ventana los recuerda tauri-plugin-window-state.
const BASE: &str = "https://www.tekpair.tech";

fn ruta_valida(cual: &str, ruta: &str) -> bool {
    let base = if cual == "tpv" { "/tpv.html" } else { "/dashboard.html" };
    ruta == base || ruta.starts_with(&format!("{base}?")) || ruta.starts_with(&format!("{base}#"))
}

fn enfocar(w: &tauri::WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

// Es async a propósito: crear una ventana desde un comando síncrono bloquea la app en Windows.
#[tauri::command]
async fn abrir_ventana(app: tauri::AppHandle, cual: String, ruta: Option<String>) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    let (label, defecto, titulo) = match cual.as_str() {
        "tpv" => ("tpv", "/tpv.html", "TekPair TPV"),
        "panel" => ("main", "/app.html", "TekPair"),
        _ => return Err("Ventana no permitida".into()),
    };
    if let Some(r) = &ruta {
        if !ruta_valida(&cual, r) {
            return Err("Ruta no permitida".into());
        }
    }
    if let Some(w) = app.get_webview_window(label) {
        let mut destino = ruta.clone();
        // La ventana del TPV se fue a otra pantalla: se devuelve al TPV.
        if destino.is_none() && label == "tpv" {
            if let Ok(u) = w.url() {
                if u.path() != "/tpv.html" && u.path() != "/tpv" {
                    destino = Some(defecto.to_string());
                }
            }
        }
        if let Some(r) = destino {
            let url: tauri::Url = format!("{BASE}{r}").parse().map_err(|e| format!("{e}"))?;
            w.navigate(url).map_err(|e| e.to_string())?;
        }
        enfocar(&w);
        return Ok(());
    }
    let url: tauri::Url = format!("{BASE}{}", ruta.as_deref().unwrap_or(defecto)).parse().map_err(|e| format!("{e}"))?;
    let mut b = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url))
        .title(titulo)
        .inner_size(1280.0, 840.0)
        .min_inner_size(900.0, 600.0);
    if label == "tpv" {
        // La web sabe que es la ventana aparte del TPV (igual que en el navegador, por su nombre).
        b = b.initialization_script("try{window.name='tekpair-tpv'}catch(e){}");
    }
    let w = b.build().map_err(|e| e.to_string())?;
    enfocar(&w);
    Ok(())
}

// ───────────────────────── Actualización automática ─────────────────────────
// Al abrir, mira la última versión publicada en GitHub (latest.json, firmada con la
// clave de TekPair). Si hay una nueva, PREGUNTA: si están cobrando, «Más tarde» y se
// vuelve a preguntar al abrir la app otra vez. La web (panel, TPV) no lo necesita:
// se carga de tekpair.tech y se actualiza sola; esto es para la parte nativa.
fn textos_actualizacion(version: &str) -> (String, String, String, String) {
    let idioma = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    let t = match idioma.get(..2).unwrap_or("es") {
        "en" => ("Update TekPair", "A new version of TekPair is available ({v}).\n\nIt installs in a few seconds and the app reopens by itself. If you are in the middle of something, choose «Later»: you will be asked again next time you open TekPair.", "Update now", "Later"),
        "fr" => ("Mettre à jour TekPair", "Une nouvelle version de TekPair est disponible ({v}).\n\nElle s'installe en quelques secondes et l'application se rouvre toute seule. Si vous êtes occupé, choisissez « Plus tard » : la question vous sera reposée à la prochaine ouverture.", "Mettre à jour", "Plus tard"),
        "it" => ("Aggiorna TekPair", "È disponibile una nuova versione di TekPair ({v}).\n\nSi installa in pochi secondi e l'app si riapre da sola. Se sei occupato, scegli «Più tardi»: te lo chiederà di nuovo la prossima volta che apri TekPair.", "Aggiorna ora", "Più tardi"),
        "de" => ("TekPair aktualisieren", "Eine neue Version von TekPair ist verfügbar ({v}).\n\nSie ist in wenigen Sekunden installiert und die App öffnet sich von selbst wieder. Wenn du gerade beschäftigt bist, wähle „Später“: Du wirst beim nächsten Öffnen erneut gefragt.", "Jetzt aktualisieren", "Später"),
        "pt" => ("Atualizar o TekPair", "Há uma nova versão do TekPair ({v}).\n\nInstala-se em poucos segundos e a app volta a abrir sozinha. Se estiver ocupado, escolha «Mais tarde»: será perguntado de novo na próxima vez que abrir o TekPair.", "Atualizar agora", "Mais tarde"),
        _ => ("Actualizar TekPair", "Hay una versión nueva de TekPair ({v}).\n\nSe instala en unos segundos y la app se vuelve a abrir sola. Si estás con un cliente, pulsa «Más tarde»: te lo volverá a preguntar la próxima vez que abras TekPair.", "Actualizar ahora", "Más tarde"),
    };
    (t.0.to_string(), t.1.replace("{v}", version), t.2.to_string(), t.3.to_string())
}

async fn buscar_actualizacion(app: tauri::AppHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;

    let Some(update) = app.updater()?.check().await? else { return Ok(()) };
    let (titulo, mensaje, si, no) = textos_actualizacion(&update.version);
    let app_dialogo = app.clone();
    let instalar = tauri::async_runtime::spawn_blocking(move || {
        app_dialogo
            .dialog()
            .message(mensaje)
            .title(titulo)
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(si, no))
            .blocking_show()
    })
    .await
    .unwrap_or(false);
    if !instalar {
        return Ok(());
    }
    // En Windows el instalador cierra la app, instala y la vuelve a abrir.
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Recuerda tamaño y posición de las ventanas (la del TPV aparte y la principal).
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = buscar_actualizacion(handle).await {
                    eprintln!("[tekpair] no se pudo comprobar la actualización: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_printers, print_raw, print_label, abrir_externo, abrir_ventana])
        .run(tauri::generate_context!())
        .expect("error al arrancar TekPair");
}
