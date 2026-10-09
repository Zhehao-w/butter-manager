fn main() {
    #[cfg(feature = "desktop")]
    {
        // Tauri tracks its config, but not changes to the embedded Windows ICO.
        println!("cargo:rerun-if-changed=icons/icon.ico");
        println!("cargo:rerun-if-changed=icons/icon.png");
        tauri_build::build();
        #[cfg(windows)]
        {
            // Tauri attaches its Common Controls v6 manifest only to binaries.
            // The hidden desktop probe needs it too (TaskDialogIndirect is absent in v5).
            let resource =
                std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("resource.lib");
            println!("cargo:rustc-link-arg-examples={}", resource.display());
        }
    }
}
