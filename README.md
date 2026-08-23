# city-builder

A city builder inspired by *Cities: Skylines*, built with [Bevy](https://bevyengine.org) 0.19.

This project starts from the official [`bevy_city` example](https://github.com/bevyengine/bevy/tree/v0.19.1/examples/large_scenes/bevy_city)
(a procedurally generated city that stresses Bevy's large-scene capabilities), ported to a
standalone crate so we can grow it into a real city builder:

- procedural grid city with roads, cars, buildings (commercial/suburban), trees, fences
- naive car traffic simulation (`simulate_cars` in `src/main.rs`)
- Feathers settings UI: car simulation, shadow maps, contact shadows, wireframe, CPU culling,
  and a "Regenerate City" button
- physical sky: atmosphere scattering, aerial perspective, bloom, TAA, contact shadows
- assets are Kenney packs loaded over HTTPS at first run and cached on disk afterwards

## Run

Requires **Rust ≥ 1.95** (Bevy 0.19 MSRV). Update your toolchain with `rustup update stable`.

```sh
cargo run                # dev build (Bevy deps are compiled at opt-level 3, see Cargo.toml)
cargo run --release      # much better frame rate, much slower to compile
```

First run needs a network connection (assets are downloaded once from
`https://github.com/bevyengine/bevy_asset_files` and cached locally). Subsequent runs are offline.

### CLI options (argh)

| Flag | Default | Meaning |
|---|---|---|
| `--seed <u64>` | `42` | City generation seed |
| `--size <u32>` | `30` | City size in blocks |
| `--no-cpu-culling` | off | Disable CPU culling on all meshes (perf stress test) |

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
  main.rs          app setup, loading screen, car simulation, atmosphere
  assets.rs        remote asset loading + car mesh merging
  generate_city.rs procedural city grid generation
  settings.rs      Feathers settings UI
```

## Roadmap ideas (from example → city builder)

- [ ] real road/zone placement tool (instead of the fixed grid)
- [ ] traffic simulation on the ECS (the example only bounces cars along a road)
- [ ] economy / population / demand simulation
- [ ] saving & loading your city

## Performance notes (ultrabook)

- `[profile.dev] opt-level = 1` + deps at `opt-level = 3` follows Bevy's fast-compiles guidance:
  first build is slow, later builds are quick, runtime stays playable.
- Capped compile parallelism if RAM is tight: `cargo build -j 4`.
- At runtime, the settings UI can disable shadow maps / contact shadows for a big frame-rate win.
