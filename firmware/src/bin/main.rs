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
    let mut grid = Grid::builder(&mut points)
        .rule(Rule::DEFAULT_RAINDROPS)
        .seed(Seed::Glider)
        .palette(Palette::Ice)
        // .two_channel(true)
        // .tint_by_field(true)
        .build();

    let mut buf = smart_led_buffer!(N_LEDS);
    let mut led = {
        let frequency = Rate::from_mhz(80);
        let rmt = Rmt::new(peripherals.RMT, frequency).expect("Failed to initialize RMT0");
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO25, &mut buf)
    };

    let delay = Delay::new();

    // ~7 generations/sec — fast enough to feel alive, slow enough to watch Life
    // evolve on a 10×15 torus.
    const FRAME_MS: u32 = 130;
    // How many recent board fingerprints to remember. A repeat within this
    // window means we've fallen into a cycle of period <= HISTORY — the boring
    // end-state (blinkers, still lifes). Period-15 oscillators escape it.
    const HISTORY: usize = 6;
    // Consecutive cyclic frames before advancing, so a brief coincidence
    // doesn't cut a pattern short (~1.5 s).
    const SETTLE: u32 = 12;
    // Hard ceiling per seed (~16 s), so genuine long-period oscillators still
    // move the slideshow along.
    const MAX_FRAMES: u32 = 120;
    let seeds = [
        Seed::Acorn,
        Seed::RPentomino,
        Seed::Lwss,
        Seed::Glider,
        Seed::Pentadecathlon,
        Seed::Toad,
        Seed::Beacon,
    ];

    let mut si = 0;
    let mut history = [0u64; HISTORY];
    let mut settled = 0u32;
    let mut on_seed = 0u32;
    loop {
        if grid.brightness() > 75 * 255 * 3 {
            panic!("Too much brightness")
        }
        led.write(grid.render()).expect("Write failed");
        grid.update();

        // Seed-cycling is a Conway concern (its patterns die into short cycles
        // on this small torus). Wildfire and Raindrops sustain themselves.
        if matches!(grid.config().rule, Rule::Conway) {
            on_seed += 1;
            let fp = grid.fingerprint();
            let cycling = history.contains(&fp);
            history[on_seed as usize % HISTORY] = fp;
            settled = if cycling { settled + 1 } else { 0 };

            if settled > SETTLE || on_seed > MAX_FRAMES {
                si = (si + 1) % seeds.len();
                grid.reseed(seeds[si]);
                history = [0u64; HISTORY];
                settled = 0;
                on_seed = 0;
                info!("Reseed → {}", si);
            }
        }

        delay.delay_millis(FRAME_MS);
    }
}
