pub use cgmath::{vec2, Point2, Vector2};
pub use image;

/// Rectangle region on display (mirrors libremarkable's mxcfb_rect).
#[derive(Copy, Clone, Debug, Default)]
pub struct mxcfb_rect {
    pub top: u32,
    pub left: u32,
    pub width: u32,
    pub height: u32,
}

impl mxcfb_rect {
    pub fn size(&self) -> Vector2<u32> {
        Vector2 {
            x: self.width,
            y: self.height,
        }
    }

    pub fn top_left(&self) -> Point2<u32> {
        Point2 {
            x: self.left,
            y: self.top,
        }
    }
}

/// Grayscale color (0 = black, 255 = white).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct color(pub u8);

#[allow(non_upper_case_globals)]
impl color {
    pub const BLACK: color = color(0);
    pub const WHITE: color = color(255);

    pub fn GRAY(v: u8) -> color {
        color(v)
    }

    pub fn as_u8(self) -> u8 {
        self.0
    }
}

/// Input event from touch or buttons.
#[derive(Clone, Debug)]
pub enum InputEvent {
    MultitouchEvent { event: MultitouchEvent },
    #[allow(dead_code)]
    GPIO { event: GPIOEvent },
}

/// Multitouch event types.
#[derive(Clone, Debug)]
pub enum MultitouchEvent {
    Press { finger: Finger },
    Release { finger: Finger },
    Move { finger: Finger },
}

/// A finger/touch point.
#[derive(Clone, Debug)]
pub struct Finger {
    pub pos: Point2<u16>,
    pub tracking_id: i32,
}

/// GPIO button event (RPP has no physical page buttons, kept for API compat).
#[derive(Clone, Debug)]
pub enum GPIOEvent {
    Press { button: PhysicalButton },
    #[allow(dead_code)]
    Release { button: PhysicalButton },
}

/// Physical button identifiers.
#[derive(Clone, Debug)]
pub enum PhysicalButton {
    LEFT,
    #[allow(dead_code)]
    MIDDLE,
    RIGHT,
    #[allow(dead_code)]
    POWER,
}
