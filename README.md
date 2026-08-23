# city-builder

A city builder built with [Bevy](https://bevyengine.org) 0.19, in the style of
Cities: Skylines.

Currently an early prototype: the scene is stripped down to a flat ground plane
so we can iterate on core mechanics. The first mechanic is **road placement** —
draw straight road segments with the mouse.

## Run

Requires **Rust 1.98.0** (pinned in `rust-toolchain.toml`; Bevy 0.19 MSRV is 1.95).

```sh
cargo run                # dev build
cargo run -- --show-fps  # with a live FPS / frame-time / entity-count overlay
```

## Controls

| Input | Action |
|---|---|
| `WASD` | Pan camera |
| `Shift` (hold) | Pan faster |
| `Q` / `E` | Rotate camera 90° (animated) |
| `Scroll wheel` | Zoom in / out |
| `C` | Toggle straight / curve road mode |
| `R` | Toggle snap to existing roads |
| `G` | Toggle snap to angles (90°) |
| `Left click` | Place road (see below) |
| `Right click` | Cancel / finalize the in-progress road |

## Road placement

Toggle between **straight** and **curve** mode with `C`. An on-screen HUD
(top-right) shows the current mode and snapping state. Switching modes mid-draw
preserves the start point and keeps any confirmed straight road segments.

- **Straight**: left-click starts a road (splitting an existing road if it lands
  on its interior, forming a T-junction). A translucent preview follows the
  cursor; each further left-click commits a segment and chains a new one from
  its end. Right-click finalizes.
- **Curve**: left-click sets the start, a second left-click sets the control
  point, and a third left-click confirms the end — a quadratic Bezier curve.
  The control point stays marked, with translucent guide lines to the start and
  end points (the bezier control polygon). Consecutive curves chain from the
  previous one's end (right-click to stop).

The cursor is a translucent gray circle with an opaque ring stroke. Two
independent snapping toggles (like Cities: Skylines) affect it: **snap to
existing roads** (`R`) and **snap to angles** (`G`, constrains straight segments
to 90° multiples). Roads are rendered as one tessellated mesh with **round joins
and caps** (via [lyon](https://lib.rs/crates/lyon)), so width stays consistent at
any turn angle and junctions/crossings join cleanly.

Invalid placements are blocked and the preview turns **red**: a segment that is
too short, overlaps an existing road, or runs too close to a parallel road (or
at a near-collinear angle to one it branches from) is refused, keeping roads
looking reasonable even in messy builds.

## CLI options (argh)

| Flag | Default | Meaning |
|---|---|---|
| `--width <px>` / `--height <px>` | `1920`/`1080` | Window resolution |
| `--pretty` | off | Enable bloom + TAA post-processing |
| `--diagnostics` | off | Log FPS / frame time / entity count every second |
| `--show-fps` | off | Draw live FPS / frame time / entity count on screen |

## Project layout

```
src/
  main.rs       app setup, orthographic camera rig (pan/rotate/zoom)
  roads.rs      ground plane + road network (lyon-tessellated chains)
  diagnostics.rs optional FPS/entity-count diagnostics + on-screen overlay
docs/
  bevy-0.19-api-notes.md   living reference of Bevy 0.19 API differences
```

## Notes

- Bevy 0.19 changes a lot vs. the widely documented 0.13–0.15 API; see
  `docs/bevy-0.19-api-notes.md` before writing Bevy code (also mirrored in
  `AGENTS.md`).
- Road rendering uses [lyon](https://lib.rs/crates/lyon) (path stroking with
  round joins/caps). Boolean union (`i_overlay`) is deferred until roads need
  per-road colors/zoning.
- Build is fast for iteration: `mold` linker (`.cargo/config.toml`) plus
  `opt-level = 1` for our code / `opt-level = 3` for deps. First build is slow
  (~17 min) but incremental builds are ~1–3 s.
