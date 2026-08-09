// VoiceBoom AI — Tauri 2.0 binary entry point
// Delegates to the library's run function

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    voiceboom_lib::run();
}
