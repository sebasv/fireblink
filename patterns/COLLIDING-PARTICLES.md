Yes, it works — and you already have most of it. `Point` is a particle: continuous f32 position, velocity (`dx`/`dy`), color, and a gaussian `scale` (blob radius), rendered additively via `point::field` with toroidal `smear`. Today they drift and pass through each other. "Colliding particles" is just adding pairwise interaction to that layer — it's orthogonal to the `Rule`/life boards.

## Sketch

**State:** reuse the `Point` array (maybe add `mass`/`radius`; `scale` already gives visual size). Keep it small — see ceiling below.

**Per frame, after `mv()`:** O(n²) pairwise check (trivial for n<8):

```
for i<j:
  dx = wrap_delta(p[j].x - p[i].x)   // torus: if >0.5 subtract 1, if <-0.5 add 1
  dy = wrap_delta(p[j].y - p[i].y)
  d2 = dx*dx + dy*dy
  if d2 < (ri+rj)^2:
     n   = (dx,dy) / sqrt(d2)                 // libm::sqrtf, already a dep
     vrel = (p[j].v - p[i].v) · n
     if vrel < 0:                             // only if approaching
        p[i].v += vrel*n;  p[j].v -= vrel*n   // equal-mass elastic: swap normal component
        // optional: dump heat at the midpoint cell → a spark via the palette
        positional_correction(p[i], p[j], n)  // nudge apart so they don't stick
```

Rendering needs zero new work: overlapping blobs already `saturating_add`, so a collision naturally flares toward white, and if you inject a `heat[mid]` spark it glows/fades through the Fire/Ice palette you already have.

## Two flavors

- **Torus (wrap):** particles wrap edges, collide midair. Cheapest, but "collisions" feel random since there are no walls.
- **Box (bounce):** reflect velocity at `x/y ∈ {0,1}` instead of wrapping. Reads much more like "particles in a box" — I'd pick this for the aesthetic. It's a per-axis sign flip, ~4 lines.

## Honest constraints on 10×15

- **Resolution isn't the blocker** — positions are continuous and gaussian-smeared, so motion stays smooth sub-pixel. The blocker is **separability**: 150 LEDs means ~3–6 particles before overlapping blobs turn to mush. Great for a few big lazy orbs knocking around; bad for a dense gas.
- **Collisions are visually brief** at any decent speed — the flare is 1–2 frames. The `FRAME_MS=130` tick actually helps here; sparks into the heat buffer (which fades over ~15 frames) give the collision a visible afterglow instead of a blink.
- Cost is negligible: a handful of `sqrtf` and dot products per frame.

**Verdict:** viable and a natural fit — box-bounce, 3–6 fat particles, collision sparks fed into the existing heat/palette pipeline. It layers on the `Point` field, so it can even run *over* a Conway/Raindrops background rather than replacing it.
