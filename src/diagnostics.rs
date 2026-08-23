//! Optional built-in Bevy diagnostics for performance profiling.
//!
//! Activated with `--diagnostics`. Periodically logs:
//! - frame time (ms) and FPS
//! - the number of tracked entities in the ECS
//! - accumulated frame count
//!
//! These tell you whether a frame is GPU-bound (frame time stays high while
//! entity/system work is small) or CPU/ECS-bound (frame time tracks entity
//! count and the heavy systems).
//!
//! This intentionally uses only Bevy's first-party, already-compiled
//! `bevy_diagnostic` crate — no extra dependencies, no rebuild of Bevy.

use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin,
};
use bevy::prelude::*;

/// Adds the profiling-only plugins to the app.
pub fn add_diagnostics(app: &mut App) {
    app.add_plugins((
        FrameTimeDiagnosticsPlugin::default(),
        EntityCountDiagnosticsPlugin::default(),
        LogDiagnosticsPlugin {
            wait_duration: std::time::Duration::from_secs(1),
            ..default()
        },
    ));
}

/// A UI node showing live FPS / frame time / entity count on screen.
#[derive(Component)]
pub struct FpsOverlay;

/// Spawns the on-screen FPS overlay.
pub fn spawn_fps_overlay(mut commands: Commands) {
    commands.spawn((
        FpsOverlay,
        Text::new("FPS: --\nFrame: -- ms\nEntities: --"),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.0, 1.0, 0.0)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: px(8.0),
            left: px(8.0),
            padding: UiRect::all(px(6.0)),
            ..default()
        },
        ZIndex(1000),
    ));
}

/// Updates the FPS overlay every frame from the diagnostics store.
pub fn update_fps_overlay(
    mut overlay: Single<&mut Text, With<FpsOverlay>>,
    diagnostics: Res<DiagnosticsStore>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let entities = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.value())
        .unwrap_or(0.0);
    overlay.0 = format!("FPS: {fps:.0}\nFrame: {frame_ms:.1} ms\nEntities: {entities:.0}");
}
