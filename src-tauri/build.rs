fn main() {
    #[cfg(feature = "desktop")]
    {
        // Tauri tracks its config, but not changes to the embedded Windows ICO.
        println!("cargo:rerun-if-changed=icons/icon.ico");
        println!("cargo:rerun-if-changed=icons/icon.png");
        tauri_build::build();
    }
}
