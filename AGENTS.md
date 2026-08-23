# AGENTS.md

Procedural minimalist city builder (Cities: Skylines-inspired, Mini
Motorways-style visuals) built on **Bevy 0.19.1**. Auto-loaded into every
session by the harness.

## Toolchain (pinned — do not change casually)

- **Rust 1.98.0** (`rust-toolchain.toml`), **Bevy 0.19.1**, edition 2024.
- `mold` linker configured in `.cargo/config.toml` (fast incremental links).
  Full build is ~17 min once; incremental `cargo build` is ~1–3 s.
- Run: `cargo build` / `cargo run -- --show-fps`. All CLI flags in `README.md`.

## Bevy 0.19 API — read `docs/bevy-0.19-api-notes.md` FIRST

Bevy 0.19 differs substantially from the widely documented 0.13–0.15 API. Before
writing any Bevy code, read `docs/bevy-0.19-api-notes.md` (the living reference,
kept in sync as new gotchas are found). The traps most likely to waste time:

- `EventReader<T>` is gone → **`MessageReader<T>`** (in `prelude`). Events derive
  **`Message`**, not `Event`.
- **`MouseWheel` is not in `prelude`** → `use bevy::input::mouse::MouseWheel;`.
- **`ScalingMode` is not in `prelude`** → `use bevy::camera::ScalingMode;`.
- `Single<&mut T>` derefs to `Mut<T>`; pattern-matching an enum needs `**single`.
- Do **not** enable `bevy/dynamic_linking` — broken on crates.io (`bevy_dylib`
  was only published through 0.17.3).

## Architecture

- Scene is fully procedural and **merged into ~16 meshes** (one per material
  color) for integrated-GPU performance — see `src/generate_city.rs`.
- Camera is a movable **orthographic** rig (Q/E rotate 90° animated, WASD pan,
  scroll zoom, 45° corner-on yaw) — see `CameraRig` / `update_camera` in
  `src/main.rs`.
- `simulate_cars` / `Road` / `Car` in `src/main.rs` are currently **dead code**
  (cars are merged into meshes, not entities) — the "Simulate Cars" toggle does
  nothing visible until cars become real entities.
