// System tray module — creates and manages the VoiceBoom tray icon and menu
// Provides quick access to settings, engine/language switching, and window control

use crate::commands;
use tauri::Emitter;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, Submenu, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

// Use the Wry runtime type alias from tauri
type Runtime = tauri::Wry;

/// Create the system tray icon with full menu
pub fn create_tray(app: &AppHandle<Runtime>) -> tauri::Result<TrayIcon<Runtime>> {
    // Menu items
    let show_hide = MenuItem::with_id(app, "toggle_window", "显示/隐藏悬浮窗", true, None::<&str>)?;

    let engine_submenu = create_engine_submenu(app)?;
    let language_submenu = create_language_submenu(app)?;

    let settings = MenuItem::with_id(app, "open_settings", "设置...", true, Some("Ctrl+,"))?;
    let about = MenuItem::with_id(app, "about", "关于 VoiceBoom", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, Some("Ctrl+Q"))?;

    // Build the menu
    let menu = Menu::with_items(
        app,
        &[
            &show_hide,
            &engine_submenu,
            &language_submenu,
            &settings,
            &about,
            &quit,
        ],
    )?;

    // Create the tray icon using the app's default icon
    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("VoiceBoom AI — 实时流式语音输入法")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event);

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    let tray = builder.build(app)?;
    Ok(tray)
}

/// Menu id prefix for engine entries; the handler emits the suffix verbatim.
const ENGINE_MENU_PREFIX: &str = "engine_";

/// Engines offered in the tray, mirroring `AVAILABLE_ENGINES` in
/// `src/constants/engines.ts`.
///
/// Only engines the backend can actually route appear here: listing an engine
/// is a promise that picking it starts that engine. The menu id is
/// `engine_<id>`, so the label and the id emitted to the frontend cannot drift
/// apart — which they did during the P0-2 rename, when the tray kept emitting
/// `openai_whisper` while that id had been reassigned to the REST provider.
const TRAY_ENGINES: &[(&str, &str)] = &[
    ("local_sense_voice", "SenseVoice（本地）"),
    ("openai_realtime", "OpenAI Whisper API"),
    ("deepgram_streaming", "Deepgram"),
];

/// Create the ASR engine selection submenu
fn create_engine_submenu(app: &AppHandle<Runtime>) -> tauri::Result<Submenu<Runtime>> {
    let mut items = Vec::with_capacity(TRAY_ENGINES.len());

    for (id, label) in TRAY_ENGINES {
        items.push(CheckMenuItem::with_id(
            app,
            format!("{ENGINE_MENU_PREFIX}{id}"),
            *label,
            true,
            *id == "local_sense_voice",
            None::<&str>,
        )?);
    }

    let mut builder = SubmenuBuilder::with_id(app, "engine", "ASR 引擎");

    for item in &items {
        builder = builder.item(item);
    }

    builder.build()
}

/// Create the language selection submenu
fn create_language_submenu(app: &AppHandle<Runtime>) -> tauri::Result<Submenu<Runtime>> {
    let auto = CheckMenuItem::with_id(app, "lang_auto", "自动检测", true, true, None::<&str>)?;
    let zh = CheckMenuItem::with_id(app, "lang_zh", "中文（普通话）", true, false, None::<&str>)?;
    let en = CheckMenuItem::with_id(app, "lang_en", "English", true, false, None::<&str>)?;
    let ja = CheckMenuItem::with_id(app, "lang_ja", "日本語", true, false, None::<&str>)?;
    let ko = CheckMenuItem::with_id(app, "lang_ko", "한국어", true, false, None::<&str>)?;

    let submenu = SubmenuBuilder::with_id(app, "language", "识别语言")
        .items(&[&auto, &zh, &en, &ja, &ko])
        .build()?;
    Ok(submenu)
}

/// Handle tray menu item clicks
fn handle_menu_event(app: &AppHandle<Runtime>, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        // Window control
        "toggle_window" => {
            toggle_floating_window(app);
        }

        // Engine selection — the suffix of the menu id *is* the engine id.
        id if id.starts_with(ENGINE_MENU_PREFIX) => {
            set_engine(app, &id[ENGINE_MENU_PREFIX.len()..]);
        }

        // Language selection
        "lang_auto" => set_language(app, "auto"),
        "lang_zh" => set_language(app, "zh"),
        "lang_en" => set_language(app, "en"),
        "lang_ja" => set_language(app, "ja"),
        "lang_ko" => set_language(app, "ko"),

        // Settings & About
        "open_settings" => {
            let app_handle = app.clone();
            let _ = commands::open_settings(app_handle);
        }
        "about" => {
            let _ = app.emit("show-about", ());
        }

        // Quit
        "quit" => {
            app.exit(0);
        }

        _ => {}
    }
}

/// Handle tray icon mouse clicks
fn handle_tray_event(tray: &TrayIcon<Runtime>, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => {
            // Left click shows and focuses the floating window.
            // Deliberately not a toggle: a stray click used to hide the main UI,
            // which reads as the app disappearing. Hiding stays on the menu item.
            let app = tray.app_handle();
            if let Some(window) = app.get_webview_window("floating") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            // Double-click opens settings
            let app_handle = tray.app_handle().clone();
            let _ = commands::open_settings(app_handle);
        }
        _ => {}
    }
}

/// Toggle the floating window visibility
fn toggle_floating_window(app: &AppHandle<Runtime>) {
    if let Some(window) = app.get_webview_window("floating") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// Set the ASR engine and emit event to frontend
fn set_engine(app: &AppHandle<Runtime>, engine: &str) {
    let _ = app.emit("tray:set-engine", engine);
}

/// Set the recognition language and emit event to frontend
fn set_language(app: &AppHandle<Runtime>, language: &str) {
    let _ = app.emit("tray:set-language", language);
}
