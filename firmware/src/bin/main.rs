#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::main;
use log::info;

use esp_hal::{delay::Delay, rmt::Rmt, time::Rate};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use patterns::{Button, ButtonState, Controls, Envelope, Grid, N_LEDS, Point, Scene, Seed};
use smart_leds::SmartLedsWrite;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Raw mic samples gathered per frame for the envelope. More = smoother, but
/// costs frame time; 64 back-to-back reads is a cheap starting point.
// ponytail: bump for a smoother envelope, or move to timer/I2S sampling if the
// electret's noise makes beat detection twitchy.
const MIC_SAMPLES: usize = 64;

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

    // Living points: `turn` curves each blob's velocity every frame so it
    // orbits, `hue_rate` walks its colour round the wheel, and `pulse` lets
    // loudness inflate the blob so the field breathes with the music.
    let mut points = [
        Point {
            x: 0.5,
            y: 0.5,
            dx: 0.020,
            dy: 0.0,
            turn: 0.06,
            hue_rate: 2,
            scale: 0.012,
            pulse: 2.0,
            ..Point::default()
        },
        Point {
            x: 0.30,
            y: 0.70,
            dx: 0.0,
            dy: 0.018,
            turn: -0.05,
            hue: 90,
            hue_rate: 3,
            scale: 0.012,
            pulse: 2.0,
            ..Point::default()
        },
        Point {
            x: 0.70,
            y: 0.30,
            dx: -0.015,
            dy: 0.010,
            turn: 0.04,
            hue: 180,
            hue_rate: 5,
            scale: 0.012,
            pulse: 2.0,
            ..Point::default()
        },
    ];

    // Start on the audio-reactive fire scene with a lively methuselah seed; the
    // buttons cycle both axes from here.
    let scene = Scene::AudioFire;
    let seed = Seed::RPentomino;
    let mut grid = Grid::builder(&mut points).config(scene.config()).build();
    grid.set_scene(scene, seed);
    let mut controls = Controls::new(scene, seed);

    // Electret mic on ADC1 (GPIO34 = ADC1_CH6, input-only). 11 dB attenuation
    // gives the full ~0–3.3 V swing an AC-coupled electret preamp produces.
    // ponytail: pin + attenuation are board assumptions — match your wiring.
    let mut adc_config = AdcConfig::new();
    let mut mic = adc_config.enable_pin(peripherals.GPIO34, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);
    let mut envelope = Envelope::new();

    // Three buttons, active-low with internal pull-ups (wire the other side to GND).
    // ponytail: GPIO32/33/27 are convenient free pins — repin to taste.
    let pull_up = InputConfig::default().with_pull(Pull::Up);
    let scene_btn = Input::new(peripherals.GPIO32, pull_up);
    let seed_btn = Input::new(peripherals.GPIO33, pull_up);
    let action_btn = Input::new(peripherals.GPIO27, pull_up);
    let mut scene_state = ButtonState::default();
    let mut seed_state = ButtonState::default();
    let mut action_state = ButtonState::default();

    let mut buf = smart_led_buffer!(N_LEDS);
    let mut led = {
        let frequency = Rate::from_mhz(80);
        let rmt = Rmt::new(peripherals.RMT, frequency).expect("Failed to initialize RMT0");
        SmartLedsAdapter::new(rmt.channel0, peripherals.GPIO25, &mut buf)
    };

    let delay = Delay::new();

    // ~50 fps: fast enough for responsive beat flares and smooth blooming.
    // Board evolution, seed cycling and brightness clipping are handled in Grid.
    let mut i = 0;
    loop {
        led.write(grid.render()).expect("Write failed");

        // Sample the mic and derive this frame's loudness + beat.
        let mut samples = [0u16; MIC_SAMPLES];
        for sample in samples.iter_mut() {
            *sample = loop {
                match adc.read_oneshot(&mut mic) {
                    Ok(value) => break value,
                    Err(_) => {} // conversion not ready yet — retry
                }
            };
        }
        let audio = envelope.frame(&samples);

        // Handle controls (active-low buttons) and advance the HUD/idle timers.
        if let Some(press) = scene_state.update(scene_btn.is_low()) {
            controls.press(&mut grid, Button::Scene, press);
        }
        if let Some(press) = seed_state.update(seed_btn.is_low()) {
            controls.press(&mut grid, Button::Seed, press);
        }
        if let Some(press) = action_state.update(action_btn.is_low()) {
            controls.press(&mut grid, Button::Action, press);
        }
        controls.tick(&mut grid);

        // ponytail: once idle (controls.idle_frames() past a threshold) and
        // controls.take_dirty() is true, persist controls.scene()/seed() to NVS
        // so the last selection survives a reboot. Skipped for now — needs
        // esp-storage and on-device tuning; add when the UX is dialed in.

        grid.update(audio);
        delay.delay_millis(20);
        i = (i + 1) % 50;
        if i == 0 {
            info!("level={} beat={}", audio.level, audio.beat);
        }
    }
}
