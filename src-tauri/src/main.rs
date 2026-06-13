// Evita que se abra una consola en Windows al lanzar la app (release).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tekpair_desktop_lib::run()
}
