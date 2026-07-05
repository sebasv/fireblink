#![cfg_attr(not(test), no_std)]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::convert::Into;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use log::info;

// use esp_hal::{Config, rmt::Rmt, time::Rate};
use esp_hal::{delay::Delay, rmt::Rmt, time::Rate};
// use esp_hal_smartled::{RmtSmartLeds, Ws2812Timing, buffer_size, color_order};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use smart_leds::{RGB8, SmartLedsWrite};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const ROWS: usize = 300;
const COLS: usize = 1;
const N_LEDS: usize = ROWS * COLS;

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32 -o esp32-wroom-32e -o unstable-hal -o ci -o zed -o esp -o esp-backtrace -o log

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let mut buf = smart_led_buffer!(N_LEDS);
    let mut led = {
        let frequency = Rate::from_mhz(80);
        let rmt = Rmt::new(peripherals.RMT, frequency).expect("Failed to initialize RMT0");
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO25, &mut buf)
    };

    let delay = Delay::new();
    let mut points = {
        [
            Point {
                x: 0.,
                y: 0.,
                dx: 0.0,
                dy: 0.01,
                color: RGB8 {
                    r: 255,
                    g: 30,
                    b: 100,
                },
            },
            Point {
                x: 1.,
                y: 0.,
                dx: -0.01,
                dy: 0.0,
                color: RGB8 {
                    r: 50,
                    g: 255,
                    b: 50,
                },
            },
        ]
    };
    let mut i = 0;
    loop {
        led.write((0..(ROWS * COLS)).map(|ix| render_points_for_led(ix, &points)))
            .expect("Write failed");
        points = points.map(|p| p.mv());
        delay.delay_millis(2);
        i = (i + 1) % 50;
        if i == 0 {
            info!("Blink!");
        }
    }
}

#[derive(Default)]
struct Point {
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    color: RGB8,
}

impl Point {
    fn mv(self) -> Point {
        Point {
            x: (self.x + self.dx) % 1.,
            y: (self.y + self.dy) % 1.,
            ..self
        }
    }
}

#[inline(always)]
fn pow(f: f32, i: usize) -> f32 {
    let mut out = f;
    for _ in 0..i {
        out *= f;
    }
    out
}

fn ix_to_grid(ix: usize) -> (usize, usize) {
    let y = ix / COLS;
    let x = if y % 2 == 0 {
        ix % COLS
    } else {
        COLS - (ix % COLS) - 1
    };
    (x, y)
}
/// map a snake to a grid:
/// 1 2 3
/// 6 5 4
/// 7 8 9
fn render_points_for_led(ix: usize, points: &[Point]) -> RGB8 {
    let mut color: RGB8 = [0, 0, 0].into();
    let (x_u, y_u) = ix_to_grid(ix);
    let y = y_u as f32 / ROWS as f32;
    let x = x_u as f32 / COLS as f32;
    for point in points {
        let dx = (point.x - x).min(1.0 - point.x.max(x) + point.x.min(x));
        let dy = (point.y - y).min(1.0 - point.y.max(y) + point.y.min(y));

        let distance = pow(dx, 2) + pow(dy, 2);
        let beta = 0.5 + 100.0 * distance;
        let beta_clipped = if beta < 1.0 { beta } else { 1.0 };
        let multiplier = pow(beta_clipped, 2) * pow(1.0 - beta_clipped, 2);
        color.g = u8::saturating_add(color.g, (point.color.g as f32 * multiplier) as u8);
        color.r = u8::saturating_add(color.r, (point.color.r as f32 * multiplier) as u8);
        color.b = u8::saturating_add(color.b, (point.color.b as f32 * multiplier) as u8);
    }
    color
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ops::Fn;

    #[test]
    fn test_ix_to_grid() {}

    #[test]
    fn test_point_renderer() {
        let mut points = [Point {
            x: 0.0,
            y: 0.0,
            dx: 0.01,
            dy: 0.01,
            color: [255, 255, 255].into(),
        }];
        fn render_grid<F: Fn(usize) -> char>(f: F) {
            for r in 0..ROWS {
                for c in 0..COLS {
                    let ix = r * COLS + if r % 2 == 0 { c } else { COLS - 1 - c };
                    let c = f(ix);
                    print!("{}", c)
                }
                print!("\n")
            }
        }
        render_grid(|ix| format!("{}", ix));
        render_grid(|ix| {
            if render_points_for_led(ix, &points).r > 125 {
                'x'
            } else {
                'o'
            }
        });
    }
}
