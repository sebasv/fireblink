# fireblink — seeds & visualization catalogue

Board: **15 wide × 10 tall**, **toroidal** (edges wrap). Coordinates are `(x, y)`
with `x ∈ 0..15`, `y ∈ 0..10`. Paste any cell list into the seed loop:

```rust
for (x, y) in SEED { conway_state[y * COLS + x] = true; }
```

## Conway seeds

Small-torus rules of thumb: **spaceships loop forever** (fly off an edge,
re-enter), **methuselahs become churning soup** that keeps the fire roaring for
hundreds of frames, **still-lifes are dull** here (static glow that fades flat).

### Glider — the classic, circles the board endlessly
```
. O .
. . O      (1,0),(2,1),(0,2),(1,2),(2,2)
O O O
```

### LWSS (lightweight spaceship) — flies sideways, wraps the 15-width forever
```
O . . O .
. . . . O      (5,3),(8,3),(9,4),(5,5),(9,5),(6,6),(7,6),(8,6),(9,6)
O . . . O
. O O O O
```

### R-pentomino — famous methuselah, erupts into ~1000 gens of chaos
```
. O O
O O .      (8,4),(9,4),(7,5),(8,5),(8,6)
. O .
```

### Acorn — 7 cells, even more explosive; fills the board with flame
```
. O . . . . .
. . . O . . .      (5,3),(7,4),(4,5),(5,5),(8,5),(9,5),(10,5)
O O . . O O O
```

### Pentadecathlon — period-15 oscillator, slow hypnotic pulse
```
. . O . . . . O . .
O O . O O O O . O O      (2,4),(7,4),(0,5),(1,5),(3,5),(4,5),(5,5),(6,5),(8,5),(9,5),(2,6),(7,6)
. . O . . . . O . .
```

### Beacon — gentle period-2 blinker
```
O O . .
O O . .      (5,3),(6,3),(5,4),(6,4),(7,5),(8,5),(7,6),(8,6)
. . O O
. . O O
```

### Toad — period-2 blinker
```
. O O O
O O O .      (6,4),(7,4),(8,4),(5,5),(6,5),(7,5)
```

### Two-glider collision — torus guarantees fireworks
Seed a glider top-left plus a second one mirrored across the board; they wrap
into each other and detonate. Start from two `Glider`s at `(1,1)` and `(11,6)`.

## Visualization ideas

Ranked by fun-per-line. #1–#3 are pure render tweaks (no new sim state); #4–#5
are bigger.

1. **Flame tint by the Point field** — multiply `ember` by the drifting point
   color already computed at that LED, so flames pick up the ambient hue
   (cool-blue fire one corner, orange another). ~3 lines in the render.
2. **Activity-reactive palette** — count births+deaths each `update()`; churn
   pushes the palette whiter/hotter, calm lets it cool to deep red. One global
   `u8` intensity scaling the palette. The panel "breathes" with the sim.
3. **Spatial hue gradient** — base hue = f(x, y), heat drives brightness; the
   same glider reads green up top, magenta at the bottom. One lookup, no state.
4. **Wildfire mode** — drop B3/S23; heat spreads to neighbours with random
   ignition/burnout. Stops being Life, becomes literal spreading fire — arguably
   more on-theme than Conway. Separate update rule.
5. **Two-channel interference** — a second independent Life board on the blue
   channel, Conway on red; overlaps glow magenta. Doubles the state array.

## Palettes (heat → colour)

- **Fire** (current) — white-hot → orange → deep red → black.
- **Ice** — white → cyan → blue → black.
- **Toxic** — white → green → dark green → black.
- **Rainbow-by-age** — hue rotates with heat instead of a fixed ramp.
