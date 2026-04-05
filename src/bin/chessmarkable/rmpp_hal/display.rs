use super::types::{color, mxcfb_rect};
use cgmath::Point2;
use drm::buffer::Buffer;
use drm::control::connector::State as ConnectorState;
use drm::control::{self, ClipRect, Device as ControlDevice};
use drm::Device;
use image::RgbImage;
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsFd, BorrowedFd};

// Embed font for text rendering
const FONT_DATA: &[u8] = include_bytes!("../../../../res/NotoSans-Regular.ttf");

/// DRM card file wrapper implementing the drm traits.
struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Device for Card {}
impl ControlDevice for Card {}

impl Card {
    fn open(path: &str) -> Self {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect(&format!("Failed to open DRM device: {}", path));
        Card(f)
    }
}

/// DRM dumb buffer display backend for reMarkable Paper Pro.
///
/// Uses a raw pointer to the mmap'd buffer to avoid self-referential
/// lifetime issues with DumbMapping<'a>.
pub struct DrmDisplay {
    card: Card,
    width: u32,
    height: u32,
    stride: u32,
    fb: control::framebuffer::Handle,
    _crtc: control::crtc::Handle,
    buffer: *mut u8,
    buffer_size: usize,
    font: fontdue::Font,
}

// Safety: the mmap'd buffer is only accessed from one thread at a time
// through &mut self methods.
unsafe impl Send for DrmDisplay {}

impl DrmDisplay {
    pub fn new() -> Self {
        let card = Card::open("/dev/dri/card0");

        // Get resources
        let res = card
            .resource_handles()
            .expect("Failed to get DRM resources");

        // Find a connected connector
        let connector = res
            .connectors()
            .iter()
            .map(|&c| card.get_connector(c, false).unwrap())
            .find(|c| c.state() == ConnectorState::Connected)
            .expect("No connected DRM connector found");

        // Get preferred mode
        let mode = connector
            .modes()
            .first()
            .expect("No display modes available")
            .clone();

        let width = mode.size().0 as u32;
        let height = mode.size().1 as u32;

        log::info!("DRM display mode: {}x{}", width, height);

        // Find an encoder + CRTC
        let encoder = connector
            .current_encoder()
            .and_then(|e| card.get_encoder(e).ok())
            .or_else(|| {
                connector
                    .encoders()
                    .iter()
                    .filter_map(|&e| card.get_encoder(e).ok())
                    .next()
            })
            .expect("No encoder found");

        let crtc_handle = encoder.crtc().unwrap_or_else(|| {
            // Pick first available CRTC
            *res.crtcs().first().expect("No CRTCs available")
        });

        // Create dumb buffer (32bpp XRGB8888)
        let mut db = card
            .create_dumb_buffer((width, height), drm_fourcc::DrmFourcc::Xrgb8888, 32)
            .expect("Failed to create dumb buffer");

        let stride = db.pitch();
        let buffer_size = db.size().0 as usize * db.size().1 as usize * 4;
        // Use the actual byte length from stride * height
        let buffer_byte_size = (stride * height) as usize;

        // Create framebuffer from dumb buffer
        let fb = card
            .add_framebuffer(&db, 24, 32)
            .expect("Failed to add framebuffer");

        // Set CRTC
        card.set_crtc(
            crtc_handle,
            Some(fb),
            (0, 0),
            &[connector.handle()],
            Some(mode),
        )
        .expect("Failed to set CRTC");

        // Map the dumb buffer — get a DumbMapping which derefs to &mut [u8]
        let mut mapping = card
            .map_dumb_buffer(&mut db)
            .expect("Failed to map dumb buffer");

        // Get a raw pointer to the buffer data so we can store it without
        // lifetime issues. The buffer stays mapped for the lifetime of DrmDisplay.
        let buffer_ptr = mapping.as_mut().as_mut_ptr();
        let actual_buffer_size = mapping.as_ref().len();

        // Leak the mapping so the mmap stays alive
        std::mem::forget(mapping);
        std::mem::forget(db);

        // Load font
        let font = fontdue::Font::from_bytes(
            FONT_DATA,
            fontdue::FontSettings::default(),
        )
        .expect("Failed to load embedded font");

        let mut display = DrmDisplay {
            card,
            width,
            height,
            stride,
            fb,
            _crtc: crtc_handle,
            buffer: buffer_ptr,
            buffer_size: actual_buffer_size,
            font,
        };

        display.clear();
        display
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.buffer, self.buffer_size) }
    }

    fn buffer_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.buffer, self.buffer_size) }
    }

    /// Set a single pixel at (x, y) to the given grayscale color.
    #[inline]
    fn set_pixel(&mut self, x: u32, y: u32, c: color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = (y * self.stride + x * 4) as usize;
        let buf = self.buffer_mut();
        if offset + 3 >= buf.len() {
            return;
        }
        let gray = c.as_u8();
        buf[offset] = gray; // B
        buf[offset + 1] = gray; // G
        buf[offset + 2] = gray; // R
        buf[offset + 3] = 0xFF; // X
    }

    /// Get a pixel's grayscale value at (x, y).
    #[inline]
    fn get_pixel(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 255;
        }
        let offset = (y * self.stride + x * 4) as usize;
        let buf = self.buffer_ref();
        if offset >= buf.len() {
            return 255;
        }
        buf[offset] // B channel (all channels are the same for grayscale)
    }

    pub fn clear(&mut self) {
        let buf = self.buffer_mut();
        // Fill with white (0xFF for all channels)
        for chunk in buf.chunks_exact_mut(4) {
            chunk[0] = 0xFF;
            chunk[1] = 0xFF;
            chunk[2] = 0xFF;
            chunk[3] = 0xFF;
        }
    }

    pub fn fill_rect(&mut self, pos: Point2<i32>, size: cgmath::Vector2<u32>, c: color) {
        let x0 = pos.x.max(0) as u32;
        let y0 = pos.y.max(0) as u32;
        let x1 = (pos.x as u32 + size.x).min(self.width);
        let y1 = (pos.y as u32 + size.y).min(self.height);
        let gray = c.as_u8();

        for y in y0..y1 {
            let row_offset = (y * self.stride) as usize;
            let buf = self.buffer_mut();
            for x in x0..x1 {
                let offset = row_offset + (x * 4) as usize;
                if offset + 3 < buf.len() {
                    buf[offset] = gray;
                    buf[offset + 1] = gray;
                    buf[offset + 2] = gray;
                    buf[offset + 3] = 0xFF;
                }
            }
        }
    }

    pub fn draw_line(&mut self, from: Point2<i32>, to: Point2<i32>, width: u32, c: color) {
        // Bresenham's line algorithm with thickness
        let dx = (to.x - from.x).abs();
        let dy = -(to.y - from.y).abs();
        let sx: i32 = if from.x < to.x { 1 } else { -1 };
        let sy: i32 = if from.y < to.y { 1 } else { -1 };
        let mut err = dx + dy;

        let mut x = from.x;
        let mut y = from.y;
        let half_w = width as i32 / 2;

        loop {
            for dy2 in -half_w..=(half_w) {
                for dx2 in -half_w..=(half_w) {
                    self.set_pixel((x + dx2) as u32, (y + dy2) as u32, c);
                }
            }

            if x == to.x && y == to.y {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn draw_rect(
        &mut self,
        pos: Point2<i32>,
        size: cgmath::Vector2<u32>,
        border_px: u32,
        c: color,
    ) {
        let x = pos.x;
        let y = pos.y;
        let w = size.x as i32;
        let h = size.y as i32;

        // Top
        self.fill_rect(
            Point2 { x, y },
            cgmath::Vector2 {
                x: size.x,
                y: border_px,
            },
            c,
        );
        // Bottom
        self.fill_rect(
            Point2 {
                x,
                y: y + h - border_px as i32,
            },
            cgmath::Vector2 {
                x: size.x,
                y: border_px,
            },
            c,
        );
        // Left
        self.fill_rect(
            Point2 { x, y },
            cgmath::Vector2 {
                x: border_px,
                y: size.y,
            },
            c,
        );
        // Right
        self.fill_rect(
            Point2 {
                x: x + w - border_px as i32,
                y,
            },
            cgmath::Vector2 {
                x: border_px,
                y: size.y,
            },
            c,
        );
    }

    pub fn draw_image(&mut self, img: &RgbImage, pos: Point2<i32>) {
        let img_w = img.width();
        let img_h = img.height();

        for iy in 0..img_h {
            let dy = pos.y + iy as i32;
            if dy < 0 || dy >= self.height as i32 {
                continue;
            }
            for ix in 0..img_w {
                let dx = pos.x + ix as i32;
                if dx < 0 || dx >= self.width as i32 {
                    continue;
                }
                let pixel = img.get_pixel(ix, iy);
                let gray = ((pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16) / 3) as u8;
                self.set_pixel(dx as u32, dy as u32, color(gray));
            }
        }
    }

    /// Read back pixel data from a region (returns RGB888 bytes, 3 bytes per pixel).
    pub fn dump_region(&self, rect: mxcfb_rect) -> Option<Vec<u8>> {
        let mut data = Vec::with_capacity((rect.width * rect.height * 3) as usize);
        for y in rect.top..(rect.top + rect.height) {
            for x in rect.left..(rect.left + rect.width) {
                let gray = self.get_pixel(x, y);
                data.push(gray);
                data.push(gray);
                data.push(gray);
            }
        }
        Some(data)
    }

    /// Render text. If `dryrun` is true, only measure and return bounding rect.
    pub fn draw_text(
        &mut self,
        pos: Point2<f32>,
        text: &str,
        size: f32,
        c: color,
        dryrun: bool,
    ) -> mxcfb_rect {
        let mut x_offset = 0.0f32;
        let mut max_height = 0u32;

        // Layout all glyphs to get metrics
        let mut glyphs: Vec<(fontdue::Metrics, Vec<u8>, f32)> = Vec::new();
        for ch in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, size);
            let glyph_x = x_offset;
            x_offset += metrics.advance_width;
            max_height = max_height.max(metrics.height as u32);
            glyphs.push((metrics, bitmap, glyph_x));
        }

        let total_width = x_offset.ceil() as u32;
        let total_height = (size * 1.2) as u32; // Approximate line height

        let rect = mxcfb_rect {
            left: pos.x as u32,
            top: pos.y.max(0.0) as u32,
            width: total_width,
            height: total_height,
        };

        if dryrun {
            return rect;
        }

        // Render glyphs
        let baseline_y = pos.y + size * 0.8; // Approximate baseline
        let gray = c.as_u8();

        for (metrics, bitmap, glyph_x) in &glyphs {
            let gx = pos.x + glyph_x + metrics.xmin as f32;
            let gy = baseline_y - metrics.height as f32 - metrics.ymin as f32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha == 0 {
                        continue;
                    }
                    let px = (gx + col as f32) as i32;
                    let py = (gy + row as f32) as i32;
                    if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
                        continue;
                    }

                    // Alpha blend with background
                    let bg = self.get_pixel(px as u32, py as u32);
                    let a = alpha as f32 / 255.0;
                    let blended = (gray as f32 * a + bg as f32 * (1.0 - a)) as u8;
                    self.set_pixel(px as u32, py as u32, color(blended));
                }
            }
        }

        rect
    }

    /// Trigger a full display refresh via DRM dirty FB ioctl.
    pub fn full_refresh(&self) -> u32 {
        let clip = ClipRect::new(0, 0, self.width as u16, self.height as u16);
        let _ = self.card.dirty_framebuffer(self.fb, &[clip]);
        0
    }

    /// Trigger a partial display refresh for a given region.
    pub fn partial_refresh(&self, region: &mxcfb_rect) -> u32 {
        let clip = ClipRect::new(
            region.left as u16,
            region.top as u16,
            (region.left + region.width).min(self.width) as u16,
            (region.top + region.height).min(self.height) as u16,
        );
        let _ = self.card.dirty_framebuffer(self.fb, &[clip]);
        0
    }
}
