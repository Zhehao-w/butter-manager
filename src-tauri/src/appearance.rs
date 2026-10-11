//! Fixed native icon matching the in-app artwork and embedded EXE resource.
#[cfg(feature = "desktop")]
pub fn icon() -> tauri::image::Image<'static> {
    let bytes: &'static [u8; 128 * 128 * 4] = include_bytes!("../icons/icon-new.rgba");
    tauri::image::Image::new(bytes, 128, 128)
}
