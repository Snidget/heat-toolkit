//! Работа с системным буфером обмена через arboard.

use arboard::Clipboard;

pub fn read_text() -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

pub fn write_text(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| e.to_string())
}

/// Скопировать растровое изображение (RGBA) в буфер обмена как картинку.
pub fn write_image_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    let image = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: std::borrow::Cow::Borrowed(rgba),
    };
    clipboard.set_image(image).map_err(|e| e.to_string())
}
