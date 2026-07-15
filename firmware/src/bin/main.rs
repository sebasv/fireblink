#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;

use esp_hal::{delay::Delay, rmt::Rmt, time::Rate};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use patterns::{Grid, N_LEDS, Palette, Point, Rule, Seed};
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
    // Three test masses orbiting the central well. Each starts offset from
    // centre with a tangential velocity (perpendicular to the radius) so it
    // orbits rather than plunges; bright heads over palette-coloured tails.
    let mut points = [
        Point {
            x: 0.5,
            y: 0.15,
            dx: 0.035,
            dy: 0.0,
            scale: 0.008,
            color: RGB8 {
                r: 90,
                g: 90,
                b: 90,
            },
        },
        Point {
            x: 0.5,
            y: 0.85,
            dx: -0.030,
            dy: 0.0,
            scale: 0.008,
            color: RGB8 {
                r: 90,
                g: 55,
                b: 20,
            },
        },
        Point {
            x: 0.20,
            y: 0.5,
            dx: 0.0,
            dy: 0.040,
            scale: 0.008,
            color: RGB8 {
                r: 40,
                g: 70,
                b: 90,
            },
        },
    ];
    // Inert substrate (empty Conway board) so the heat buffer carries only the
    // comet trails; gravity drives the particle field on top.
    let mut grid = Grid::builder(&mut points)
        .rule(Rule::Conway)
        .seed(Seed::Empty)
        .palette(Palette::Fire)
        .gravity(true)
        .build();

    let mut buf = smart_led_buffer!(N_LEDS);
    let mut led = {
        let frequency = Rate::from_mhz(80);
        let rmt = Rmt::new(peripherals.RMT, frequency).expect("Failed to initialize RMT0");
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO25, &mut buf)
    };

    let delay = Delay::new();

    // ~7 generations/sec — fast enough to feel alive, slow enough to watch Life
    // evolve on a 10×15 torus. Seed cycling and brightness clipping are handled
    // inside the Grid.
    const FRAME_MS: u32 = 130;
    loop {
        led.write(grid.render()).expect("Write failed");
        grid.update();

        delay.delay_millis(FRAME_MS);
    }
}
