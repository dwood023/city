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
| `Left click` | Start / confirm (and chain) a road segment |
| `Right click` | Cancel the in-progress road segment |

## Road placement

- A translucent gray **circle** (diameter = road width) marks the cursor on the
  ground. When the cursor is within snap range of an existing road, it snaps to
  the nearest point on that road.
- **Left click** starts a segment; a translucent road preview is drawn from the
  start point to the cursor.
- Each **further left click** commits the preview as an opaque road and chains
  a new segment from its end, so consecutive segments can be placed quickly.
- **Right click** exits back to neutral without committing the current preview.

All roads are currently straight segments.

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
  roads.rs      ground plane + road placement mechanic
  diagnostics.rs optional FPS/entity-count diagnostics + on-screen overlay
docs/
  bevy-0.19-api-notes.md   living reference of Bevy 0.19 API differences
```

## Notes

- Bevy 0.19 changes a lot vs. the widely documented 0.13–0.15 API; see
  `docs/bevy-0.19-api-notes.md` before writing Bevy code (also mirrored in
  `AGENTS.md`).
- Build is fast for iteration: `mold` linker (`.cargo/config.toml`) plus
  `opt-level = 1` for our code / `opt-level = 3` for deps. First build is slow
  (~17 min) but incremental builds are ~1–3 s.
