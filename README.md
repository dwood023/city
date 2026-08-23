# city-builder

A minimalist, procedurally generated city built with [Bevy](https://bevyengine.org) 0.19.

Originally forked from the official [`bevy_city` example](https://github.com/bevyengine/bevy/tree/v0.19.1/examples/large_scenes/bevy_city)
(a heavily detailed stress test), this version was rebuilt around a hard
performance constraint: it must run smoothly on an **integrated GPU** (Intel UHD 620).

To achieve that, the scene is now **fully procedural and minimalist** (a
Mini Motorways-style look): every building, tree, car, and fence is a small set
of **shared colored primitives** (cubes, cylinders, spheres). Because all
instances of a given prop share the same mesh and material, Bevy batches them
into a tiny number of draw calls, and the GPU only ever holds a handful of
meshes — avoiding the shared-memory thrash the detailed glTF asset version hit.

- procedural grid city with ground, roads, buildings, trees, fences, cars
- naive car traffic simulation
- Feathers settings UI: simulate cars, wireframe, CPU culling, and a
  "Regenerate City" button
- optional on-screen FPS / frame-time / entity-count overlay

## Run

Requires **Rust ≥ 1.95** (Bevy 0.19 MSRV). Update your toolchain with `rustup update stable`.

```sh
cargo run                # dev build — minimalist city, ~45 FPS on an ultrabook iGPU
cargo run -- --release   # better frame rate, slower to compile (see README note)
```

No network connection is needed — all geometry is generated at runtime.

### CLI options (argh)

| Flag | Default | Meaning |
|---|---|---|
| `--seed <u64>` | `42` | City generation seed |
| `--size <u32>` | `12` | City size in blocks (larger sizes push the scene into shared-memory thrash on an iGPU) |
| `--low-graphics` | off | Disable bloom + TAA for a large GPU win |
| `--no-cars` | off | Disable cars + traffic simulation |
| `--no-buildings` | off | Disable buildings (keeps roads, ground, trees) |
| `--no-decorations` | off | Disable trees/fences (keeps ground, roads, buildings, cars) |
| `--minimal` | off | Only ground tiles + roads (minimal skeleton) |
| `--width <px>` / `--height <px>` | `1920`/`1080` | Window resolution |
| `--diagnostics` | off | Log FPS / frame time / entity count every second |
| `--show-fps` | off | Draw live FPS / frame time / entity count on screen (top-left) |

### Controls (free camera)

| Input | Action |
|---|---|
| `WASD` | Move |
| `E` / `Q` | Up / down |
| `Shift` (hold) | Run (faster) |
| `Right mouse` (hold) | Look around |
| `M` | Toggle cursor grab |
| `Scroll wheel` | Adjust movement speed |

## Project layout

```
src/
  main.rs          app setup, camera, car simulation
  generate_city.rs procedural city generation (shared meshes/materials, spawn logic)
  settings.rs      Feathers settings UI
  diagnostics.rs   optional FPS/entity-count diagnostics + on-screen overlay
```

## Performance notes (ultrabook)

- `[profile.dev] opt-level = 1` + deps at `opt-level = 3` follows Bevy's fast-compiles guidance:
  first build is slow, later builds are quick, runtime stays playable.
- Capped compile parallelism if RAM is tight: `cargo build -j 4`.
- Heavy GPU features default off; `--low-graphics` strips bloom/TAA for the biggest win.
- Run `--show-fps` to watch live FPS / entity counts and confirm your frame budget.
