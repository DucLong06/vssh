//! vssh desktop: the window. Everything it can do is in the library.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vssh_desktop::run()
}
