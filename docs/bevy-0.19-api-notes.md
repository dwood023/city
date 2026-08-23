# Bevy 0.19 API notes

Living reference of Bevy 0.19.1 API differences and gotchas discovered while
building this project. Bevy 0.19 has many breaking changes vs. the commonly
documented 0.13–0.15 API, so these are recorded here to avoid re-discovering
them. Each entry states what changed and the correct 0.19 form.

Environment: **Rust 1.98.0**, **Bevy 0.19.1**, **edition 2024** (pinned in
`rust-toolchain.toml`).

## Events

- **`EventReader<T>` is gone** — renamed to **`MessageReader<T>`** (in `prelude`).
  ```rust
  // 0.19 (not EventReader):
  fn sys(mut scroll: MessageReader<MouseWheel>) {
      for e in scroll.read() { /* ... */ }
  }
  ```
- Events now derive **`Message`** instead of `Event`:
  ```rust
  #[derive(Message, Debug, Clone, Copy)]
  pub struct MyEvent { /* ... */ }
  ```
- **`MouseWheel` is NOT in `prelude`** — import it explicitly:
  ```rust
  use bevy::input::mouse::MouseWheel;
  ```
  Fields: `unit: MouseScrollUnit`, `x: f32`, `y: f32`, `window: Entity`,
  `phase: TouchPhase`.
- There is also an **`AccumulatedMouseScroll`** resource (`delta: Vec2`,
  `unit`), summed each frame and reset to zero by the input plugin — useful
  when you only want per-frame totals.

## System params

- **`Single<&mut T>` derefs to `Mut<T>`**, not `&mut T`. Field access
  (`x.foo = ...`) and method calls auto-deref fine, but **pattern-matching an
  enum needs a double deref**:
  ```rust
  // Single<&mut Projection, With<CityCamera>>
  if let Projection::Orthographic(ref mut ortho) = **projection { /* ... */ }
  ```
  `Single<T>` (owned/`Copy` data like `Single<Entity>`) derefs to `T` directly
  (`*single` is the value).

## Camera / projection

- `Projection` and `OrthographicProjection` are in `prelude`.
- **`ScalingMode` is NOT in `prelude`** — import it:
  ```rust
  use bevy::camera::ScalingMode;
  ```
  `ScalingMode` variants in 0.19: `WindowSize` (default), `Fixed { width,
  height }`, `AutoMin { min_width, min_height }`, `AutoMax { max_width,
  max_height }`, `FixedVertical { viewport_height }`, `FixedHorizontal {
  viewport_width }`.
- `OrthographicProjection::default_3d()` exists (near `0.0`, far `1000.0`,
  scale `1.0`, `ScalingMode::WindowSize`). `default_2d()` sets near `-1000.0`.
- `Transform::look_at(&mut self, target, up)` is available; the builder
  `Transform::from_translation(p).looking_at(t, up)` also still exists.
- `TemporalAntiAliasing` is a struct — construct with `TemporalAntiAliasing::default()`.

## Linking / build

- **`bevy/dynamic_linking` is broken for 0.19.1 on crates.io.** It pulls in the
  `bevy_dylib` crate, which was only ever published up to **0.17.3** — there is
  no 0.19.x, so resolution fails. Do not enable it; use the `mold` linker
  instead (`.cargo/config.toml`) for fast incremental links.
- `bevy` meta-crate has **no `bevy_diagnostic` feature**; add
  `bevy_diagnostic = "0.19.1"` as a direct dependency. The diagnostic plugins
  are structs: `FrameTimeDiagnosticsPlugin::default()`,
  `EntityCountDiagnosticsPlugin::default()`. `FrameCountPlugin` is already added
  by `DefaultPlugins`.
- Free-camera controller lives behind the **`free_camera`** feature at
  `bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin,
  FreeCameraState}`.

## UI / window

- `WindowResolution::new` takes **`u32`** width/height (not `f32`).
- `ZIndex` is a **separate component** (`ZIndex(1000)`), not a field on `Node`.
- UI padding needs `UiRect::all(px(6.0))` (or `UiRect::axes`/`edges`), not a
  bare `px(6.0)`.

## Scene notation (`bsn!`)

- `bsn!` / `bsn_list!` build `Scene` / `SceneList`; spawn with `.spawn()` in a
  `Startup` system.
- `bsn!` does **not** support `..default()` spread inside struct literals; write
  full struct values or use `template_value(...)`.
- Components with values are written as bare expressions (`Msaa::Off`,
  `Projection::Orthographic(...)`), defaulted components as bare identifiers.

## Misc

- `StaticTransformOptimizations::Enabled` is a resource you insert to opt into
  static transform optimizations.
- `PresentMode::AutoVsync` is the vsync present mode for a stable frame rate on
  integrated GPUs.
