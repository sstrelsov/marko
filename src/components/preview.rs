use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use image::DynamicImage;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

use crate::markdown;
use crate::theme;

/// A decoded image sent from a background thread to the main thread.
pub struct DecodedImage {
    pub path: PathBuf,
    pub image: Option<DynamicImage>,
    /// Relative URL for pre-populating file_cache (e.g. ".marko/images/screenshot-XXX.png").
    pub url_hint: Option<String>,
}

/// A clickable link region in the rendered preview buffer.
pub struct ClickableLink {
    pub y: u16,
    pub x_start: u16,
    pub x_end: u16,
    pub url: String,
}

/// Cached resized image ready for half-block rendering.
struct ResizedImage {
    rgba: image::RgbaImage,
    /// Dimensions this was resized for (cols, pixel_rows).
    target_w: u32,
    target_h: u32,
}

pub struct PreviewState {
    pub scroll_offset: u16,
    pub content_height: u16,
    /// Clickable link regions from the last render.
    pub click_links: Vec<ClickableLink>,
    /// Cache: image URL → local file path (None = failed to fetch/not fetchable).
    file_cache: HashMap<String, Option<PathBuf>>,
    /// Cache: file path → decoded DynamicImage (None = failed to decode).
    image_decode_cache: HashMap<PathBuf, Option<DynamicImage>>,
    /// Cache: file path → resized RGBA at specific dimensions (avoids per-frame resize).
    resize_cache: HashMap<PathBuf, ResizedImage>,
    /// Screen area used during last render.
    last_area: Rect,
    /// Sender for background decode threads to deliver decoded images.
    image_tx: mpsc::Sender<DecodedImage>,
    /// Receiver drained in poll_decoded_images() (~10fps from tick()).
    image_rx: mpsc::Receiver<DecodedImage>,
    /// Paths currently being decoded in background threads (prevents duplicate spawns).
    decoding_in_flight: HashSet<PathBuf>,
    /// Graphics protocol picker (Sixel/Kitty/iTerm2). None = half-block fallback only.
    picker: Option<Picker>,
    /// Cache: file path → StatefulProtocol for graphics protocol rendering.
    protocol_cache: HashMap<PathBuf, Box<StatefulProtocol>>,
    /// Paths that were rendered via graphics protocol last frame (for cleanup).
    last_gfx_paths: HashSet<PathBuf>,
}

impl PreviewState {
    pub fn new() -> Self {
        let (image_tx, image_rx) = mpsc::channel();
        Self {
            scroll_offset: 0,
            content_height: 0,
            click_links: Vec::new(),
            file_cache: HashMap::new(),
            image_decode_cache: HashMap::new(),
            resize_cache: HashMap::new(),
            last_area: Rect::default(),
            image_tx,
            image_rx,
            decoding_in_flight: HashSet::new(),
            picker: Picker::from_query_stdio().ok(),
            protocol_cache: HashMap::new(),
            last_gfx_paths: HashSet::new(),
        }
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16, viewport_height: u16) {
        let max_scroll = self.content_height.saturating_sub(viewport_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn page_up(&mut self, viewport_height: u16) {
        self.scroll_up(viewport_height.saturating_sub(2));
    }

    pub fn page_down(&mut self, viewport_height: u16) {
        self.scroll_down(viewport_height.saturating_sub(2), viewport_height);
    }

    /// Find the URL at a given screen position, if any.
    pub fn url_at(&self, x: u16, y: u16) -> Option<&str> {
        for link in &self.click_links {
            if link.y == y && x >= link.x_start && x < link.x_end {
                return Some(&link.url);
            }
        }
        None
    }

    /// Returns a clone of the sender for background threads to deliver decoded images.
    pub fn image_sender(&self) -> mpsc::Sender<DecodedImage> {
        self.image_tx.clone()
    }

    /// Drains all pending decoded images from background threads.
    /// Call from tick() to pick up results without blocking.
    pub fn poll_decoded_images(&mut self) {
        while let Ok(msg) = self.image_rx.try_recv() {
            self.decoding_in_flight.remove(&msg.path);
            // Invalidate caches so next render re-processes
            self.resize_cache.remove(&msg.path);
            self.protocol_cache.remove(&msg.path);
            self.image_decode_cache.insert(msg.path.clone(), msg.image);
            // Pre-populate file_cache so resolve_image_path() isn't needed
            if let Some(url) = msg.url_hint {
                self.file_cache.insert(url, Some(msg.path));
            }
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, content: &str, state: &mut PreviewState, base_dir: &Path) {
    let rendered = markdown::renderer::render_markdown(content, area.width.saturating_sub(2) as usize);

    state.content_height = rendered.text.lines.len() as u16;

    if state.last_area.width != area.width || state.last_area.height != area.height {
        state.protocol_cache.clear();
    }
    state.last_area = area;

    let link_urls = rendered.link_urls;

    // Collect image info before moving rendered.text into Paragraph
    let image_infos = rendered.image_infos;

    let paragraph = Paragraph::new(rendered.text)
        .style(theme::editor_style())
        .scroll((state.scroll_offset, 0));

    frame.render_widget(paragraph, area);

    // Resolve, cache, and resize images; collect render jobs
    struct ImageJob {
        rect: Rect,
        y_offset: u16,
        path: PathBuf,
        full_cols: u16,
    }
    let mut jobs: Vec<ImageJob> = Vec::new();
    for info in &image_infos {
        let text_line = info.start_line as u16;
        let end_line = text_line + info.line_count as u16;

        if end_line <= state.scroll_offset || text_line >= state.scroll_offset + area.height {
            continue;
        }

        let file_path = match state.file_cache.get(&info.url) {
            Some(cached) => cached.clone(),
            None => {
                let resolved = resolve_image_path(&info.url, base_dir);
                // Only cache successful resolutions — None may become Some
                // once a background thread finishes writing the file.
                if resolved.is_some() {
                    state.file_cache.insert(info.url.clone(), resolved.clone());
                }
                resolved
            }
        };

        if let Some(path) = file_path {
            // Non-blocking: if not yet decoded, spawn background thread and skip this frame
            if !state.image_decode_cache.contains_key(&path) {
                if !state.decoding_in_flight.contains(&path) {
                    state.decoding_in_flight.insert(path.clone());
                    let tx = state.image_tx.clone();
                    let decode_path = path.clone();
                    std::thread::spawn(move || {
                        let img = load_image(&decode_path);
                        if let Some(ref i) = img {
                            save_thumbnail(i, &decode_path);
                        }
                        let _ = tx.send(DecodedImage {
                            path: decode_path,
                            image: img,
                            url_hint: None,
                        });
                    });
                }
                continue; // skip this image until decode finishes
            }

            let full_cols = area.width.saturating_sub(1);
            let full_rows = info.line_count as u16;

            // Pre-compute resized RGBA (only when dimensions change)
            let target_w = full_cols as u32;
            let target_h = (full_rows * 2) as u32;
            let needs_resize = state.resize_cache.get(&path).map_or(true, |cached| {
                cached.target_w != target_w || cached.target_h != target_h
            });
            if needs_resize {
                if let Some(Some(ref img)) = state.image_decode_cache.get(&path) {
                    use image::imageops::FilterType;
                    // Use fast Triangle filter for large images (>2MP) since
                    // we're downscaling to terminal cells anyway.
                    let pixels = img.width() as u64 * img.height() as u64;
                    let filter = if pixels > 2_000_000 {
                        FilterType::Triangle
                    } else {
                        FilterType::Lanczos3
                    };
                    let resized = img.resize(target_w, target_h, filter);
                    let rgba = resized.to_rgba8();
                    state.resize_cache.insert(path.clone(), ResizedImage {
                        rgba,
                        target_w,
                        target_h,
                    });
                }
            }

            // Use signed arithmetic so images partially above the viewport
            // correctly compute y_offset (rows clipped from top).
            let screen_y = area.y as i32 + text_line as i32 - state.scroll_offset as i32;
            let visible_top = screen_y.max(area.y as i32) as u16;
            let visible_bottom = (screen_y + full_rows as i32).min((area.y + area.height) as i32) as u16;
            if visible_top < visible_bottom {
                jobs.push(ImageJob {
                    rect: Rect::new(
                        area.x,
                        visible_top,
                        full_cols,
                        visible_bottom - visible_top,
                    ),
                    y_offset: (visible_top as i32 - screen_y) as u16,
                    path,
                    full_cols,
                });
            }
        }
    }

    // Render images. When a graphics protocol picker is available, use it for
    // full-resolution rendering. Images that are partially scrolled off the top
    // (y_offset > 0) are hidden entirely — the placeholder box shows instead.
    // When no picker is available, fall back to half-block rendering everywhere.
    let has_picker = state.picker.is_some();
    let mut this_frame_gfx: HashSet<PathBuf> = HashSet::new();

    for job in &jobs {
        if has_picker && job.y_offset == 0 {
            // Graphics protocol: full-res, image top is within viewport
            if !state.protocol_cache.contains_key(&job.path) {
                if let Some(Some(ref img)) = state.image_decode_cache.get(&job.path) {
                    if let Some(ref picker) = state.picker {
                        let protocol = picker.new_resize_protocol(img.clone());
                        state.protocol_cache.insert(job.path.clone(), Box::new(protocol));
                    }
                }
            }
            // Clear cells so placeholder doesn't show through
            {
                let buf = frame.buffer_mut();
                for y in job.rect.y..job.rect.y + job.rect.height {
                    for x in job.rect.x..job.rect.x + job.rect.width {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.reset();
                        }
                    }
                }
            }
            if let Some(protocol) = state.protocol_cache.get_mut(&job.path) {
                frame.render_stateful_widget(StatefulImage::default(), job.rect, protocol.as_mut());
                this_frame_gfx.insert(job.path.clone());
            }
        } else if has_picker {
            // y_offset > 0: image partially scrolled off top — hide it entirely
            // but still clear cells so the placeholder text doesn't show through.
            let buf = frame.buffer_mut();
            for y in job.rect.y..job.rect.y + job.rect.height {
                for x in job.rect.x..job.rect.x + job.rect.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.reset();
                    }
                }
            }
        } else {
            // No graphics protocol available: half-block fallback with cropping
            let buf = frame.buffer_mut();
            if let Some(cached) = state.resize_cache.get(&job.path) {
                render_halfblock_image(buf, job.rect, &cached.rgba, job.full_cols, job.y_offset);
            }
        }
    }

    // Any image that was rendered via graphics protocol last frame but NOT this
    // frame needs its protocol dropped so the terminal clears the placement.
    for old_path in state.last_gfx_paths.difference(&this_frame_gfx) {
        state.protocol_cache.remove(old_path);
    }
    state.last_gfx_paths = this_frame_gfx;

    // Build clickable link regions
    build_link_regions(frame, area, &link_urls, &mut state.click_links);

    // Scrollbar
    if state.content_height > area.height {
        let mut scrollbar_state = ScrollbarState::new(state.content_height as usize)
            .position(state.scroll_offset as usize)
            .viewport_content_length(area.height as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme::LINE_NUMBER))
            .track_style(Style::default().fg(theme::BORDER));
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

/// Composite an RGBA pixel over a background color using alpha blending.
#[inline]
fn blend(pixel: &image::Rgba<u8>, bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let a = pixel[3] as u16;
    let inv_a = 255 - a;
    (
        ((pixel[0] as u16 * a + bg.0 as u16 * inv_a) / 255) as u8,
        ((pixel[1] as u16 * a + bg.1 as u16 * inv_a) / 255) as u8,
        ((pixel[2] as u16 * a + bg.2 as u16 * inv_a) / 255) as u8,
    )
}

/// Render a pre-resized RGBA image into the buffer using half-block Unicode characters.
/// Each cell shows two vertical pixels: upper pixel as fg color, lower as bg color.
/// Preserves aspect ratio centering within the rect.
/// `full_cols` is the full image area width (for centering math).
/// `y_offset` is the number of rows clipped from the top (due to scrolling).
fn render_halfblock_image(
    buf: &mut Buffer,
    rect: Rect,
    rgba: &image::RgbaImage,
    full_cols: u16,
    y_offset: u16,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let bg = (30u8, 30u8, 30u8);
    let img_w = rgba.width();
    let img_h = rgba.height();

    // Center horizontally within the full column width
    let x_pad = (full_cols as u32).saturating_sub(img_w) / 2;

    for dy in 0..rect.height {
        let img_row = dy + y_offset;
        for dx in 0..rect.width {
            let img_x = (dx as u32).wrapping_sub(x_pad);
            let upper_y = (img_row * 2) as u32;
            let lower_y = upper_y + 1;

            let (ur, ug, ub) = if img_x < img_w && upper_y < img_h {
                blend(rgba.get_pixel(img_x, upper_y), bg)
            } else {
                bg
            };
            let (lr, lg, lb) = if img_x < img_w && lower_y < img_h {
                blend(rgba.get_pixel(img_x, lower_y), bg)
            } else {
                bg
            };

            if let Some(cell) = buf.cell_mut((rect.x + dx, rect.y + dy)) {
                let is_bg = (ur, ug, ub) == bg && (lr, lg, lb) == bg;
                if is_bg {
                    cell.reset();
                } else {
                    cell.set_symbol("\u{2580}") // ▀
                        .set_fg(Color::Rgb(ur, ug, ub))
                        .set_bg(Color::Rgb(lr, lg, lb));
                }
            }
        }
    }
}

/// Returns the path for a pre-computed thumbnail of the given image.
/// e.g. `/path/to/screenshot-123.png` → `/path/to/screenshot-123.thumb.png`
fn thumbnail_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
    path.with_file_name(format!("{}.thumb.png", stem))
}

/// Saves a downscaled thumbnail alongside the original for fast reload.
/// Skips if the image is already small enough that decoding is fast.
pub(crate) fn save_thumbnail(img: &DynamicImage, original_path: &Path) {
    let pixels = img.width() as u64 * img.height() as u64;
    if pixels <= 640_000 {
        return; // already small, thumbnail not needed
    }
    let thumb = thumbnail_path(original_path);
    let max_dim = 800u32;
    let resized = img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle);
    let _ = resized.save(&thumb);
}

/// Load an image file and return a DynamicImage. Handles PNG, JPEG, GIF, BMP.
/// For SVG/SVGZ, uses resvg. Checks for a pre-computed thumbnail first
/// so large images (e.g. retina screenshots) load in milliseconds on reload.
pub(crate) fn load_image(path: &std::path::Path) -> Option<DynamicImage> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "svg" || ext == "svgz" {
        return load_svg(path);
    }

    // Check for pre-computed thumbnail first (much faster for large images)
    let thumb = thumbnail_path(path);
    if thumb.exists() {
        if let Some(img) = load_image_raw(&thumb) {
            return Some(img);
        }
    }

    load_image_raw(path)
}

/// Low-level image decode from a file path (no thumbnail lookup).
fn load_image_raw(path: &Path) -> Option<DynamicImage> {
    // Guess format from file content (not extension) so mismatched files
    // (e.g. TIFF data with .png extension from macOS clipboard) still load.
    let reader = image::ImageReader::open(path).ok()?;
    let reader = reader.with_guessed_format().ok()?;
    reader.decode().ok()
}

/// Decode an image from raw bytes (PNG, TIFF, etc.) without touching disk.
/// Used by the paste thread to avoid a redundant file→decode round-trip.
pub(crate) fn load_image_from_bytes(bytes: &[u8]) -> Option<DynamicImage> {
    use std::io::Cursor;
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.decode().ok()
}

/// Render SVG to a DynamicImage using resvg (pure Rust, no external tools).
/// Renders at a higher resolution than the SVG's native size for better quality
/// when downscaled to terminal cells.
fn load_svg(path: &std::path::Path) -> Option<DynamicImage> {
    let svg_data = std::fs::read(path).ok()?;
    let tree = resvg::usvg::Tree::from_data(&svg_data, &Default::default()).ok()?;
    let size = tree.size();
    if size.width() == 0.0 || size.height() == 0.0 {
        return None;
    }
    // Render at a larger size for better detail when downscaled
    let target_w = 800u32;
    let scale = target_w as f32 / size.width();
    let target_h = (size.height() * scale) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    image::RgbaImage::from_raw(target_w, target_h, pixmap.take()).map(DynamicImage::ImageRgba8)
}

/// Open a URL in the system default browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
}

/// Scan the rendered buffer for link-styled cell runs and map them to URLs.
fn build_link_regions(
    frame: &mut Frame,
    area: Rect,
    link_urls: &[String],
    out: &mut Vec<ClickableLink>,
) {
    out.clear();
    if link_urls.is_empty() {
        return;
    }

    let link_fg = theme::link_style().fg;
    let buf = frame.buffer_mut();
    let mut url_index = 0;
    let mut in_link = false;
    let mut x_start = 0u16;

    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell((x, y)) {
                let is_link = cell.style().fg == link_fg
                    && cell.style().add_modifier.contains(Modifier::UNDERLINED);

                if is_link && !in_link {
                    in_link = true;
                    x_start = x;
                } else if !is_link && in_link {
                    in_link = false;
                    if url_index < link_urls.len() {
                        out.push(ClickableLink {
                            y,
                            x_start,
                            x_end: x,
                            url: link_urls[url_index].clone(),
                        });
                        url_index += 1;
                    }
                }
            }
        }
        if in_link {
            in_link = false;
            if url_index < link_urls.len() {
                out.push(ClickableLink {
                    y,
                    x_start,
                    x_end: area.x + area.width,
                    url: link_urls[url_index].clone(),
                });
                url_index += 1;
            }
        }
    }
}

/// Resolve an image URL to a local file path.
/// Downloads remote images via curl; returns None if unavailable.
fn resolve_image_path(url: &str, base_dir: &Path) -> Option<PathBuf> {
    if url.starts_with("http://") || url.starts_with("https://") {
        fetch_remote_image(url)
    } else {
        let path = PathBuf::from(url);
        // Try as-is (absolute path), then relative to the markdown file's directory
        let candidate = if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        };
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    }
}

/// Fetch a remote image via curl, caching in a temp directory.
fn fetch_remote_image(url: &str) -> Option<PathBuf> {
    let cache_dir = std::env::temp_dir().join("marko_images");
    std::fs::create_dir_all(&cache_dir).ok()?;

    // Preserve file extension for format detection
    let ext = url.rsplit('.').next().unwrap_or("png");
    let ext = if ext.len() <= 4 && ext.chars().all(|c| c.is_alphanumeric()) {
        ext
    } else {
        "png"
    };
    let key: String = url
        .chars()
        .filter(|c| c.is_alphanumeric())
        .rev()
        .take(50)
        .collect();
    let cache_path = cache_dir.join(format!("{}.{}", key, ext));

    if cache_path.exists() && std::fs::metadata(&cache_path).ok()?.len() > 0 {
        return Some(cache_path);
    }

    let status = std::process::Command::new("curl")
        .args(["-s", "-L", "--max-time", "10", "-o"])
        .arg(&cache_path)
        .arg(url)
        .status()
        .ok()?;

    if status.success() && cache_path.exists() && std::fs::metadata(&cache_path).ok()?.len() > 0 {
        Some(cache_path)
    } else {
        let _ = std::fs::remove_file(&cache_path);
        None
    }
}
