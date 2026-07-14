# fireblink — mic-reactive ideas

A mic turns this from a screensaver into something reactive. This menu is tuned
to what's cheap on an ESP32 and to the mode system already in `lib.rs`
(`Config`, `Rule`, `Palette`, the heat buffer, `activity`, `HEAT_DECAY`,
`reseed`). Most ideas are a single scalar per frame threaded into pieces that
already exist.

## Audio visualization (sound → pixels)

### Envelope / loudness — no FFT, start here
- **Global brightness pump** — RMS amplitude scales overall brightness; the
  board breathes with volume. One multiply in `render_led` (reuse the
  `palette::scale` helper).
- **VU meter** — fill LEDs proportional to loudness, green→red. The snake `ix`
  order already makes a natural bar.
- **Beat-driven heat injection** — detect onsets (amplitude jump above a
  running average); on each beat re-ignite `heat_a` for a burst of cells, or
  drop a live glider via `reseed`. Music literally sets the fire alight —
  the most on-theme option.

### Frequency — needs a small FFT (32–64 bins via `microfft`)
- **Spectrum analyzer** — map bins across the 15 columns, magnitude → column
  heat. Classic, gorgeous with the fire palette.
- **Bass → Point.scale** — feed low-band energy into `Point.scale` so blobs
  bloom on the kick and shrink between beats.
- **Band-split colour** — bass→R, mids→G, treble→B intensity; the board's
  colour tracks the spectral balance.
- **Spectral centroid → hue** — bright/dull sound rotates the palette; plugs
  straight into `Palette::RainbowByAge`.

## Audio as controller (sound → parameters/modes)

- **Beat → generation tick** — advance the board one `update()` per beat
  instead of every 20 ms. The simulation dances on tempo. Cheap and striking.
- **Loudness → sim speed** — louder = faster update cadence; quiet crawls.
- **Amplitude → `HEAT_DECAY`** — loud = long comet trails, silence = quick
  fade. Directly modulates a knob already exposed.
- **Clap/whistle to cycle modes** — a clap transient calls `reseed(next_seed)`;
  a sustained whistle cycles `Palette`. Hands-free mode switching that
  *complements the 3-button UX* (see the button proposal) rather than competing
  for buttons.
- **Silence → screensaver** — below threshold for N seconds, drift back to the
  ambient Point field only; sound wakes the board.

## Plumbing into the current structure

Compute the audio features once per frame, pass them into the sim as one small
input rather than scattering ADC reads through the render path:

```rust
#[derive(Default, Clone, Copy)]
pub struct Audio {
    pub level: u8,   // RMS envelope, auto-gained to 0..=255
    pub beat: bool,  // onset detected this frame
}

// update() takes the frame's audio; render stays pure over Grid state
pub fn update(&mut self, audio: Audio) { … }
```

- **loudness/beat** → new `Audio` arg to `update()`, plus `Config` toggles
  (`audio_brightness`, `audio_decay`, `beat_tick`) so each reaction is
  independently selectable, same pattern as the viz flags.
- **beat detection** → treat "react to beats" as a scene in the button UX, not
  a new orthogonal flag, to keep the combinatorial space sane.
- **spectrum → columns** → a render path that reads bins blended with (or
  instead of) `heat_a`; only worth it once the envelope pipeline is solid.

## Recommended first cut

**RMS envelope only, no FFT: beat-triggered heat injection + amplitude→decay.**
Zero DSP, immediately fun, reuses the heat buffer, and proves the mic signal
path before investing in an FFT. Add the spectrum analyzer later.

## Hardware notes

- **Mic:** prefer an I2S MEMS mic (e.g. INMP441/ICS-43434) over an analog
  electret + ADC — cleaner signal, no ADC-noise fight, and the ESP32 I2S
  peripheral DMAs samples for you. Analog works but you'll spend the savings on
  filtering.
- **Auto-gain / noise-gate, not fixed thresholds.** Mic modules vary wildly in
  gain and DC bias and the room noise floor drifts. Track a running mean/variance
  and gate off that, or beat detection is deaf in a quiet room and trigger-happy
  in a loud one. This is the calibration a minimal model can't see — budget for it.
- **Sampling:** ~8–16 kHz is plenty for envelope + beat; a 32–64-point FFT at
  that rate gives usable low-mid-high bands without stealing the frame budget.
