#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::panic;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use log::info;

use esp_hal::{delay::Delay, rmt::Rmt, time::Rate};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use patterns::{COLS, Grid, N_LEDS, Point};
use smart_leds::{RGB8, SmartLedsWrite};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

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
    let mut points = [
        // Point {
        //     x: 0.,
        //     y: 0.,
        //     dx: 0.0,
        //     dy: 0.0001,
        //     scale: 0.1,
        //     color: RGB8 {
        //         r: 255,
        //         g: 30,
        //         b: 100,
        //     },
        // },
        Point {
            x: 0.,
            y: 0.,
            dx: 0.001,
            dy: 0.001,
            scale: 0.01,
            color: RGB8 { r: 1, g: 5, b: 1 },
        },
    ];
    let mut conway_state = [false; N_LEDS];
    // glider, top-left corner (x, y):
    // . X .
    // . . X
    // X X X
    for (x, y) in [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)] {
        conway_state[y * COLS + x] = true;
    }
    let mut grid = Grid::new(&mut points, conway_state);

    let mut buf = smart_led_buffer!(N_LEDS);
    let mut led = {
        let frequency = Rate::from_mhz(80);
        let rmt = Rmt::new(peripherals.RMT, frequency).expect("Failed to initialize RMT0");
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO25, &mut buf)
    };

    let delay = Delay::new();

    let mut i = 0;
    loop {
        if grid.brightness() > 75 * 255 * 3 {
            panic!("Too much brightness")
        }
        led.write(grid.render()).expect("Write failed");
        grid.update();
        delay.delay_millis(20);
        i = (i + 1) % 50;
        if i == 0 {
            info!("Blink!");
        }
    }
}
