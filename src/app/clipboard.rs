//! System clipboard integration: copy, paste text, and paste images.
//!
//! On macOS, uses NSPasteboard to grab raw PNG bytes directly for fast
//! image paste (~100ms vs ~10s with decode/re-encode).

use super::*;

impl<'a> App<'a> {
    // ─── Clipboard helpers ───────────────────────────────────────────────
    // arboard::Clipboard is created on demand (not stored in App — it's not Send
    // and creating it is cheap).

    /// Writes text to the system clipboard via arboard.
    pub(super) fn copy_to_clipboard(&self, text: &str) {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            let _ = clip.set_text(text.to_string());
        }
    }

    /// Reads text from the system clipboard. Returns None on failure.
    pub(super) fn paste_from_clipboard(&self) -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    /// Returns a markdown image link immediately and spawns a background
    /// thread that saves the clipboard image as a PNG file.
    ///
    /// On macOS, uses NSPasteboard to grab raw PNG bytes directly — no
    /// decode/re-encode needed, so the file appears in ~100ms instead of ~10s.
    ///
    /// The background thread also sends the decoded `DynamicImage` through the
    /// preview channel so the first render doesn't block on a redundant decode.
    pub(super) fn paste_image_from_clipboard(&self) -> Option<String> {
        let parent = self.file_path.parent()?;
        let images_dir = parent.join(".marko").join("images");
        std::fs::create_dir_all(&images_dir).ok()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let filename = format!("screenshot-{}.png", now.as_secs());
        let file_path = images_dir.join(&filename);
        let relative_url = format!(".marko/images/{}", filename);
        let md_text = format!("![screenshot]({})\n", relative_url);

        let image_tx = self.preview.image_sender();
        let url_hint = relative_url.clone();

        std::thread::spawn(move || {
            use std::io::Write;
            let log = |msg: &str| {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/marko-debug.log")
                {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    let _ = writeln!(f, "[{:.3}] [paste_image] {}", ts.as_secs_f64(), msg);
                }
            };

            let send_image = |img: Option<image::DynamicImage>| {
                if let Some(ref i) = img {
                    crate::components::preview::save_thumbnail(i, &file_path);
                }
                let _ = image_tx.send(crate::components::preview::DecodedImage {
                    path: file_path.clone(),
                    image: img,
                    url_hint: Some(url_hint.clone()),
                });
            };

            if let Some(raw_bytes) = clipboard_png_bytes() {
                log(&format!("got clipboard bytes: {} bytes", raw_bytes.len()));
                // macOS often provides TIFF even when asked for PNG — check magic bytes
                let is_png = raw_bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
                if is_png {
                    log("data is actual PNG, writing directly");
                    match std::fs::write(&file_path, &raw_bytes) {
                        Ok(_) => log("PNG saved (raw)"),
                        Err(e) => log(&format!("write failed: {}", e)),
                    }
                    let img = crate::components::preview::load_image_from_bytes(&raw_bytes);
                    send_image(img);
                } else {
                    log("data is TIFF, transcoding to PNG");
                    let img = transcode_to_png(&raw_bytes, &file_path, &log);
                    send_image(img);
                }
            } else {
                log("no image data on clipboard, falling back to arboard");
                save_clipboard_image_arboard(&file_path, &log);
                let img = crate::components::preview::load_image(&file_path);
                send_image(img);
            }
        });

        Some(md_text)
    }
}

/// Grabs raw PNG bytes directly from the macOS pasteboard (no decode).
#[cfg(target_os = "macos")]
fn clipboard_png_bytes() -> Option<Vec<u8>> {
    use objc2::rc::Retained;
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    use objc2_foundation::NSData;

    let pasteboard: Option<Retained<NSPasteboard>> =
        unsafe { objc2::msg_send![NSPasteboard::class(), generalPasteboard] };
    let pasteboard = pasteboard?;

    // Try PNG first (already the right format), fall back to TIFF
    let data: Retained<NSData> =
        unsafe { pasteboard.dataForType(NSPasteboardTypePNG) }
            .or_else(|| unsafe { pasteboard.dataForType(NSPasteboardTypeTIFF) })?;

    Some(unsafe { data.as_bytes_unchecked() }.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn clipboard_png_bytes() -> Option<Vec<u8>> {
    None
}

/// Decodes image bytes (TIFF, etc.), re-encodes as PNG, and returns the decoded image.
fn transcode_to_png(raw_bytes: &[u8], file_path: &std::path::Path, log: &dyn Fn(&str)) -> Option<image::DynamicImage> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use std::io::Cursor;

    let reader = match image::ImageReader::new(Cursor::new(raw_bytes)).with_guessed_format() {
        Ok(r) => r,
        Err(e) => {
            log(&format!("format guess failed: {}", e));
            return None;
        }
    };
    let img = match reader.decode() {
        Ok(i) => i,
        Err(e) => {
            log(&format!("decode failed: {}", e));
            return None;
        }
    };
    log(&format!("decoded to {}x{}", img.width(), img.height()));
    let file = match std::fs::File::create(file_path) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("file create failed: {}", e));
            return Some(img);
        }
    };
    let encoder = PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Fast,
        FilterType::Sub,
    );
    match img.write_with_encoder(encoder) {
        Ok(_) => log("PNG saved (transcoded)"),
        Err(e) => log(&format!("PNG encode failed: {}", e)),
    }
    Some(img)
}

/// Fallback: use arboard to decode clipboard image, then re-encode as PNG.
fn save_clipboard_image_arboard(file_path: &std::path::Path, log: &dyn Fn(&str)) {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};

    let mut clip = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log(&format!("Clipboard::new failed: {}", e));
            return;
        }
    };
    let img_data = match clip.get_image() {
        Ok(d) => d,
        Err(e) => {
            log(&format!("get_image failed: {}", e));
            return;
        }
    };
    let Some(rgba_image) = image::RgbaImage::from_raw(
        img_data.width as u32,
        img_data.height as u32,
        img_data.bytes.into_owned(),
    ) else {
        log("RgbaImage::from_raw returned None");
        return;
    };
    let file = match std::fs::File::create(file_path) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("file create failed: {}", e));
            return;
        }
    };
    let encoder = PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Fast,
        FilterType::Sub,
    );
    if let Err(e) = rgba_image.write_with_encoder(encoder) {
        log(&format!("PNG encode failed: {}", e));
        return;
    }
    log("PNG saved (arboard fallback)");
}
