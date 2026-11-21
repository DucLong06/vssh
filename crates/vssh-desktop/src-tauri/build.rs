fn main() {
    let cfg = |name: &str| std::env::var(format!("CARGO_CFG_TARGET_{name}")).unwrap_or_default();
    let mut attributes = tauri_build::Attributes::new();
    // Tauri puts its manifest (Common Controls 6) into the app alone, and a
    // test binary without it does not start on Windows
    // (STATUS_ENTRYPOINT_NOT_FOUND). So the linker embeds it, into every
    // binary of this crate: what Tauri's own build.rs does for its tests.
    if cfg("OS") == "windows" && cfg("ENV") == "msvc" {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={manifest}");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{manifest}");
        let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        attributes = attributes.windows_attributes(windows);
    }
    tauri_build::try_build(attributes).expect("the build script of tauri failed");
}
