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
- **`PrimaryWindow` is not in `prelude`** → `use bevy::window::PrimaryWindow;`.
- `Single<&mut T>` derefs to `Mut<T>`; pattern-matching an enum needs `**single`.
- Mouse picking: `Camera::viewport_to_world(&GlobalTransform, pos)` → `Ray3d`;
  run the system in `PostUpdate` after `TransformSystems::Propagate`.
- Do **not** enable `bevy/dynamic_linking` — broken on crates.io (`bevy_dylib`
  was only published through 0.17.3).

## Architecture

- Scene is a flat ground plane (`#2e3d2a`) plus player-placed **straight road
  segments** — see `src/roads.rs`.
- Roads are a set of **polylines ("chains")** (`RoadNetwork`). Committed chains
  are stroked with **lyon** (round joins + round caps) into a **single merged
  mesh**, giving consistent road width at any turn angle and clean T-junctions
  and crossings. Starting a road on another road's interior splits that chain at
  the junction (a T-junction node).
- Two placement modes (toggle `C`): **straight** (click-to-start, left-click
  commits-and-chains, right-click finalizes) and **curve** (three clicks = start,
  control point, end — a quadratic Bezier sampled into a polyline). Consecutive
  curves chain from the previous end. The preview is a tessellated ribbon
  rebuilt each frame into one reused mesh handle; the curve's control point and
  translucent guide lines (control polygon) are shown while placing.
- Independent snapping toggles (like CS): **snap to roads** (`R`) and **snap to
  angles** (`G`, constrains straight segments to 90° multiples, `ANGLE_SNAP_STEP`).
- **Placement guards** (preview turns red when blocked): segments too short
  (`MIN_ROAD_LENGTH`), overlapping an existing road, or running too close to a
  parallel/near-collinear road (`MIN_CLEARANCE`, `PARALLEL_DOT`, `MIN_OVERLAP`)
  are refused. Crossings and angled branches are allowed.
- `i_overlay` union is deferred until roads need per-road colors/zoning (opaque
  same-color overlap is visually correct now).
- Camera is a movable **orthographic** rig (Q/E rotate 90° animated, WASD pan,
  Shift = faster pan, scroll zoom, 45° corner-on yaw) — see `CameraRig` /
  `update_camera` in `src/main.rs`.
- The procedural city (`generate_city.rs`), car sim, and settings UI were
  removed to focus on mechanics; they remain in git history (`e2c85b7` and
  earlier) if needed.
