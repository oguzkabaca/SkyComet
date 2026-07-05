// Prevents a console window from appearing alongside the app window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    skycomet_lib::run();
}
