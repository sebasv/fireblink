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
use patterns::{Blend, Grid, N_LEDS, Palette, Point, Rule, Seed};
use smart_leds::SmartLedsWrite;

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
    // No drifting field points for this demo — the picture is purely the two
    // life channels.
    let mut points: [Point; 0] = [];
    // Channel A: a Conway glider over channel B's raindrop ripples. Hue-balance
    // blend puts the glider (A) at the warm end and rain (B) at the cool end;
    // overlaps slide through green instead of blowing out to white. Flip
    // `.blend(Blend::Add)` to compare against the additive Fire/Ice version.
    let mut grid = Grid::builder(&mut points)
        .rule(Rule::Conway { seed: Seed::Glider })
        .rule_b(Rule::DEFAULT_RAINDROPS)
        .palette(Palette::Fire)
        .palette_b(Palette::Ice)
        .blend(Blend::HueBalance)
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
