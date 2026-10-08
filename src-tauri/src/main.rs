// Keeps a console window from opening behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    d8_shuffler_lib::run()
}
