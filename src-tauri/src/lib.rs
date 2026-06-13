// TekPair Desktop — carcasa nativa que carga https://www.tekpair.tech y expone
// impresión NATIVA a la impresora del sistema, cross-platform.
//
// La web detecta que corre dentro de Tauri (window.__TAURI__) y llama a:
//   - list_printers() -> ["EPSON TM-T20", "POS-80", "Brother QL-800", ...]
//   - print_raw(printer, data) -> envía esos bytes tal cual a la impresora (ESC/POS,
//     ZPL, EPL... lenguaje de etiquetas/tickets). Silencioso, usa el driver del sistema.
//
// Windows: winspool (RAW). macOS/Linux: CUPS vía `lp -o raw` y `lpstat`.
//
// NOTA: las etiquetas HTML actuales de TekPair se imprimen con el diálogo del
// navegador. La impresión HTML *silenciosa* (render -> PDF -> impresora) es la
// fase 2 y se añadirá como comando print_pdf().

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_printers, print_raw])
        .run(tauri::generate_context!())
        .expect("error al arrancar TekPair");
}
