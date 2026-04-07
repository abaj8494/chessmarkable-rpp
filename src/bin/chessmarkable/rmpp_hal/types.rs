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

/// RGB color. The RPP Gallery 3 e-ink display supports color via QTFB RGB888.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[allow(non_upper_case_globals)]
impl color {
    pub const BLACK: color = color { r: 0, g: 0, b: 0 };
    pub const WHITE: color = color { r: 255, g: 255, b: 255 };

    pub fn GRAY(v: u8) -> color {
        color { r: v, g: v, b: v }
    }

    // Chess board colors (warm brown tones, lichess-inspired)
    pub const LIGHT_SQUARE: color = color { r: 240, g: 217, b: 181 };
    pub const DARK_SQUARE: color = color { r: 181, g: 136, b: 99 };
    pub const SELECTED_SQ: color = color { r: 246, g: 246, b: 105 };
    pub const LAST_MOVE_LIGHT: color = color { r: 205, g: 210, b: 106 };
    pub const LAST_MOVE_DARK: color = color { r: 170, g: 162, b: 58 };
    pub const MOVE_HINT_LIGHT: color = color { r: 170, g: 210, b: 130 };
    pub const MOVE_HINT_DARK: color = color { r: 130, g: 170, b: 90 };
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
