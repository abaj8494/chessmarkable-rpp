use super::types::{Finger, InputEvent, MultitouchEvent};
use cgmath::Point2;
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::Sender;
use std::thread;

// Linux input event constants
const EV_SYN: u16 = 0x00;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;

// Touch input device path on RPP
const TOUCH_DEVICE: &str = "/dev/input/event3";

// Raw input_event struct (matches kernel struct input_event for aarch64)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct InputEventRaw {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

const INPUT_EVENT_SIZE: usize = std::mem::size_of::<InputEventRaw>();

/// Maximum number of simultaneous touch slots
const MAX_SLOTS: usize = 10;

#[derive(Clone, Debug, Default)]
struct TouchSlot {
    tracking_id: i32,
    x: i32,
    y: i32,
    active: bool,
    changed: bool,
}

/// Query the absinfo for an axis to get min/max range.
/// Falls back to reasonable defaults if ioctl fails.
fn get_abs_range(fd: i32, axis: u16) -> (i32, i32) {
    #[repr(C)]
    #[derive(Default)]
    struct AbsInfo {
        value: i32,
        minimum: i32,
        maximum: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    // EVIOCGABS ioctl number: _IOR('E', 0x40 + axis, struct input_absinfo)
    // For aarch64: direction=2 (read), size=24, type='E'=0x45, nr=0x40+axis
    let size = std::mem::size_of::<AbsInfo>() as u64;
    let nr = 0x40u64 + axis as u64;
    let ioctl_num: u64 = (2u64 << 30) | (size << 16) | (0x45u64 << 8) | nr;

    let mut info = AbsInfo::default();
    let ret = unsafe {
        libc::ioctl(fd, ioctl_num as libc::c_ulong as libc::Ioctl, &mut info as *mut AbsInfo)
    };

    if ret < 0 {
        log::warn!("Failed to get absinfo for axis {}, using defaults", axis);
        (0, 4096) // Reasonable default for touch input
    } else {
        (info.minimum, info.maximum)
    }
}

/// Start input reader thread for touch events.
pub fn start_input_threads(tx: Sender<InputEvent>, display_width: u32, display_height: u32) {
    thread::spawn(move || {
        let mut file = match File::open(TOUCH_DEVICE) {
            Ok(f) => f,
            Err(e) => {
                log::error!("Failed to open touch device {}: {}", TOUCH_DEVICE, e);
                return;
            }
        };

        // Get touch coordinate ranges for scaling
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        let (x_min, x_max) = get_abs_range(fd, ABS_MT_POSITION_X);
        let (y_min, y_max) = get_abs_range(fd, ABS_MT_POSITION_Y);
        log::info!(
            "Touch range: X={}..{}, Y={}..{}, display={}x{}",
            x_min, x_max, y_min, y_max, display_width, display_height
        );

        let x_range = (x_max - x_min) as f64;
        let y_range = (y_max - y_min) as f64;

        let mut slots = vec![TouchSlot::default(); MAX_SLOTS];
        for slot in &mut slots {
            slot.tracking_id = -1;
        }
        let mut current_slot: usize = 0;

        let mut buf = [0u8; INPUT_EVENT_SIZE];
        loop {
            match file.read_exact(&mut buf) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Error reading touch device: {}", e);
                    break;
                }
            }

            let event: InputEventRaw = unsafe { std::ptr::read(buf.as_ptr() as *const _) };

            match event.type_ {
                EV_ABS => match event.code {
                    ABS_MT_SLOT => {
                        current_slot = event.value as usize;
                        if current_slot >= MAX_SLOTS {
                            current_slot = 0;
                        }
                    }
                    ABS_MT_TRACKING_ID => {
                        let slot = &mut slots[current_slot];
                        if event.value == -1 {
                            // Finger lifted
                            slot.active = false;
                            slot.changed = true;
                        } else {
                            // New finger
                            slot.tracking_id = event.value;
                            slot.active = true;
                            slot.changed = true;
                        }
                    }
                    ABS_MT_POSITION_X => {
                        slots[current_slot].x = event.value;
                        slots[current_slot].changed = true;
                    }
                    ABS_MT_POSITION_Y => {
                        slots[current_slot].y = event.value;
                        slots[current_slot].changed = true;
                    }
                    _ => {}
                },
                EV_SYN if event.code == SYN_REPORT => {
                    // Emit events for changed slots
                    for slot in slots.iter_mut() {
                        if !slot.changed {
                            continue;
                        }
                        slot.changed = false;

                        // Scale raw coordinates to display pixels
                        let scaled_x =
                            ((slot.x - x_min) as f64 / x_range * display_width as f64) as u16;
                        let scaled_y =
                            ((slot.y - y_min) as f64 / y_range * display_height as f64) as u16;

                        let finger = Finger {
                            pos: Point2 {
                                x: scaled_x,
                                y: scaled_y,
                            },
                            tracking_id: slot.tracking_id,
                        };

                        let event = if !slot.active {
                            InputEvent::MultitouchEvent {
                                event: MultitouchEvent::Release { finger },
                            }
                        } else {
                            InputEvent::MultitouchEvent {
                                event: MultitouchEvent::Press { finger },
                            }
                        };

                        let _ = tx.send(event);
                    }
                }
                _ => {}
            }
        }
    });
}
