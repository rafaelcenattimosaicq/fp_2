// hide the console window
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cloud_desktop_lib::run();
}
