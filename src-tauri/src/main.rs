// Evita abrir um console extra no Windows em builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    whats_recap_lib::run()
}
