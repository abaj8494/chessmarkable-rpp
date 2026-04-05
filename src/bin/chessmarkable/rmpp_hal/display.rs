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
/// The RPP's Gallery 3 e-ink panel uses a packed grayscale format:
/// each byte of the XRGB8888 DRM buffer represents one physical
/// grayscale pixel. So a DRM mode of 405x1084 at 32bpp (stride=1620)
/// actually gives us a 1620x1084 grayscale framebuffer.
pub struct DrmDisplay {
    card: Card,
    /// Physical pixel width (= DRM stride in bytes, typically DRM_width * 4)
    phys_width: u32,
    /// Physical pixel height (= DRM mode height)
    phys_height: u32,
    /// DRM stride in bytes (= phys_width for packed format)
    stride: u32,
    /// DRM mode width (for ClipRect calculations)
    drm_width: u32,
    /// DRM mode height
    drm_height: u32,
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

        let drm_width = mode.size().0 as u32;
        let drm_height = mode.size().1 as u32;

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
            *res.crtcs().first().expect("No CRTCs available")
        });

        // Create dumb buffer (32bpp XRGB8888)
        let mut db = card
            .create_dumb_buffer((drm_width, drm_height), drm_fourcc::DrmFourcc::Xrgb8888, 32)
            .expect("Failed to create dumb buffer");

        let stride = db.pitch();

        // RPP packed pixel format: each byte = one grayscale pixel
        // Physical resolution = stride × drm_height
        let phys_width = stride;
        let phys_height = drm_height;

        log::info!(
            "DRM mode: {}x{}, stride: {}, physical pixels: {}x{}",
            drm_width, drm_height, stride, phys_width, phys_height
        );

        // Create framebuffer from dumb buffer
        let fb = card
            .add_framebuffer(&db, 24, 32)
            .expect("Failed to add framebuffer");

        // Set CRTC — may fail if another process holds DRM master
        match card.set_crtc(
            crtc_handle,
            Some(fb),
            (0, 0),
            &[connector.handle()],
            Some(mode),
        ) {
            Ok(_) => log::info!("CRTC set successfully"),
            Err(e) => {
                log::warn!("set_crtc failed ({}), trying page_flip instead", e);
                if let Err(e2) = card.page_flip(
                    crtc_handle,
                    fb,
                    drm::control::PageFlipFlags::empty(),
                    None,
                ) {
                    log::warn!("page_flip also failed ({}), display may not update", e2);
                }
            }
        }

        // Map the dumb buffer
        let mut mapping = card
            .map_dumb_buffer(&mut db)
            .expect("Failed to map dumb buffer");

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
            phys_width,
            phys_height,
            stride,
            drm_width,
            drm_height,
            fb,
            _crtc: crtc_handle,
            buffer: buffer_ptr,
            buffer_size: actual_buffer_size,
            font,
        };

        display.clear();
        display
    }

    /// Logical display width in physical pixels.
    pub fn width(&self) -> u32 {
        self.phys_width
    }

    /// Logical display height in physical pixels.
    pub fn height(&self) -> u32 {
        self.phys_height
    }

    fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.buffer, self.buffer_size) }
    }

    fn buffer_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.buffer, self.buffer_size) }
    }

    /// Set a single physical pixel at (x, y) to the given grayscale color.
    /// In the RPP packed format, each byte in the buffer is one pixel.
    #[inline]
    fn set_pixel(&mut self, x: u32, y: u32, c: color) {
        if x >= self.phys_width || y >= self.phys_height {
            return;
        }
        // Each byte = one grayscale pixel. Row stride = self.stride bytes.
        let offset = (y * self.stride + x) as usize;
        let buf = self.buffer_mut();
        if offset < buf.len() {
            buf[offset] = c.as_u8();
        }
    }

    /// Get a pixel's grayscale value at (x, y).
    #[inline]
    fn get_pixel(&self, x: u32, y: u32) -> u8 {
        if x >= self.phys_width || y >= self.phys_height {
            return 255;
        }
        let offset = (y * self.stride + x) as usize;
        let buf = self.buffer_ref();
        if offset < buf.len() {
            buf[offset]
        } else {
            255
        }
    }

    pub fn clear(&mut self) {
        let buf = self.buffer_mut();
        // Fill with white (0xFF)
        buf.fill(0xFF);
    }

    pub fn fill_rect(&mut self, pos: Point2<i32>, size: cgmath::Vector2<u32>, c: color) {
        let x0 = pos.x.max(0) as u32;
        let y0 = pos.y.max(0) as u32;
        let x1 = ((pos.x.max(0) as u32) + size.x).min(self.phys_width);
        let y1 = ((pos.y.max(0) as u32) + size.y).min(self.phys_height);
        let gray = c.as_u8();

        let stride = self.stride;
        let buf = self.buffer_mut();
        for y in y0..y1 {
            let row_start = (y * stride + x0) as usize;
            let row_end = (y * stride + x1) as usize;
            if row_end <= buf.len() {
                buf[row_start..row_end].fill(gray);
            }
        }
    }

    pub fn draw_line(&mut self, from: Point2<i32>, to: Point2<i32>, width: u32, c: color) {
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
            cgmath::Vector2 { x: size.x, y: border_px },
            c,
        );
        // Bottom
        self.fill_rect(
            Point2 { x, y: y + h - border_px as i32 },
            cgmath::Vector2 { x: size.x, y: border_px },
            c,
        );
        // Left
        self.fill_rect(
            Point2 { x, y },
            cgmath::Vector2 { x: border_px, y: size.y },
            c,
        );
        // Right
        self.fill_rect(
            Point2 { x: x + w - border_px as i32, y },
            cgmath::Vector2 { x: border_px, y: size.y },
            c,
        );
    }

    pub fn draw_image(&mut self, img: &RgbImage, pos: Point2<i32>) {
        let img_w = img.width();
        let img_h = img.height();

        for iy in 0..img_h {
            let dy = pos.y + iy as i32;
            if dy < 0 || dy >= self.phys_height as i32 {
                continue;
            }
            for ix in 0..img_w {
                let dx = pos.x + ix as i32;
                if dx < 0 || dx >= self.phys_width as i32 {
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

        let mut glyphs: Vec<(fontdue::Metrics, Vec<u8>, f32)> = Vec::new();
        for ch in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, size);
            let glyph_x = x_offset;
            x_offset += metrics.advance_width;
            max_height = max_height.max(metrics.height as u32);
            glyphs.push((metrics, bitmap, glyph_x));
        }

        let total_width = x_offset.ceil() as u32;
        let total_height = (size * 1.2) as u32;

        let rect = mxcfb_rect {
            left: pos.x as u32,
            top: pos.y.max(0.0) as u32,
            width: total_width,
            height: total_height,
        };

        if dryrun {
            return rect;
        }

        let baseline_y = pos.y + size * 0.8;
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
                    if px < 0 || py < 0
                        || px >= self.phys_width as i32
                        || py >= self.phys_height as i32
                    {
                        continue;
                    }

                    let bg = self.get_pixel(px as u32, py as u32);
                    let a = alpha as f32 / 255.0;
                    let blended = (gray as f32 * a + bg as f32 * (1.0 - a)) as u8;
                    self.set_pixel(px as u32, py as u32, color(blended));
                }
            }
        }

        rect
    }

    /// Convert physical pixel x coordinate to DRM ClipRect coordinate.
    /// In packed format, 4 physical pixels = 1 DRM pixel.
    #[inline]
    fn phys_to_drm_x(&self, x: u32) -> u16 {
        (x / 4).min(self.drm_width) as u16
    }

    /// Trigger a full display refresh via DRM dirty FB ioctl.
    pub fn full_refresh(&self) -> u32 {
        let clip = ClipRect::new(0, 0, self.drm_width as u16, self.drm_height as u16);
        let _ = self.card.dirty_framebuffer(self.fb, &[clip]);
        0
    }

    /// Trigger a partial display refresh for a given region.
    /// Region coordinates are in physical pixels; we convert to DRM pixel coords.
    pub fn partial_refresh(&self, region: &mxcfb_rect) -> u32 {
        let x1 = self.phys_to_drm_x(region.left);
        let y1 = region.top.min(self.drm_height) as u16;
        let x2 = self.phys_to_drm_x(region.left + region.width);
        let y2 = (region.top + region.height).min(self.drm_height) as u16;
        let clip = ClipRect::new(x1, y1, x2, y2);
        let _ = self.card.dirty_framebuffer(self.fb, &[clip]);
        0
    }
}
