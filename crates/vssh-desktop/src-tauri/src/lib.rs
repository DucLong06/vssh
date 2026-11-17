//! vssh desktop: a Tauri shell over `vssh-core`. This crate is the thin
//! layer between the window and the core; it never names the terminal app.

/// Opens the window and runs until it is closed.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("vssh could not start its window");
}
