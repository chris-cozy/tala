fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "dispatch",
            "pick_file",
            "save_file",
            "open_external",
        ]),
    ))
    .expect("Tauri build configuration");
}
