//! A road-placement prototype on a flat ground plane.
//!
//! The scene is stripped down to a single flat ground plane (`#2e3d2a`) so we
//! can iterate on mechanics. The player places straight road segments with the
//! mouse (see `roads`). The camera is a movable orthographic rig (Cities:
//! Skylines style): Q/E rotate 90°, WASD pans, scroll zooms.

use argh::FromArgs;
use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    camera::ScalingMode,
    input::mouse::MouseWheel,
    post_process::bloom::Bloom,
    prelude::*,
    transform::TransformSystems,
    window::{PresentMode, WindowResolution},
    winit::WinitSettings,
};

mod diagnostics;
mod roads;

#[derive(FromArgs, Resource, Clone)]
/// Config
pub struct Args {
    /// enable per-second logging of frame time / FPS / entity count (profiling)
    #[argh(switch)]
    diagnostics: bool,

    /// window width in pixels (default 1920)
    #[argh(option, default = "1920")]
    width: u32,

    /// window height in pixels (default 1080)
    #[argh(option, default = "1080")]
    height: u32,

    /// enable bloom + TAA (off by default; full-screen post effects are the
    /// biggest GPU cost on integrated graphics)
    #[argh(switch)]
    pretty: bool,

    /// draw FPS / frame time / entity count on screen (top-left)
    #[argh(switch)]
    show_fps: bool,
}

fn main() {
    let args: Args = argh::from_env();

    let mut app = App::new();
    if args.diagnostics || args.show_fps {
        diagnostics::add_diagnostics(&mut app);
    }

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "city-builder".into(),
            resolution: WindowResolution::new(args.width, args.height)
                .with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoVsync,
            position: WindowPosition::Centered(MonitorSelection::Primary),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(args.clone())
    .insert_resource(ClearColor(Color::srgb(0.55, 0.8, 0.95)))
    .insert_resource(WinitSettings::continuous())
    .insert_resource(StaticTransformOptimizations::Enabled)
    .init_resource::<CameraRig>()
    .add_systems(Startup, (spawn_camera, roads::setup_roads, roads::spawn_hud))
    .add_systems(Update, (update_camera, roads::update_hud))
    .add_systems(
        PostUpdate,
        roads::road_placement_system.after(TransformSystems::Propagate),
    );

    if args.pretty {
        app.add_systems(Startup, apply_pretty);
    }
    if args.show_fps {
        app.add_systems(Startup, diagnostics::spawn_fps_overlay)
            .add_systems(Update, diagnostics::update_fps_overlay);
    }

    app.run();
}

/// Movable orthographic camera rig (Cities: Skylines style). The projection is
/// orthographic and the yaw is permanently offset 45° so we always look at the
/// *corners* of blocks, never straight down a road. Q/E rotate the view 90°
/// (animated), WASD pans, and the scroll wheel zooms.
#[derive(Resource)]
struct CameraRig {
    /// World point the camera looks at (panning moves this; y stays 0).
    focus: Vec3,
    /// Smoothed yaw (radians), eased toward `target_yaw` every frame.
    current_yaw: f32,
    /// Target yaw, stepped in 90° increments. Kept unbounded so rotation always
    /// continues in the pressed direction instead of snapping the shortest way.
    target_yaw: f32,
    /// Zoom level: 1.0 is the default framing, higher is more zoomed in.
    zoom: f32,
    /// Orthographic view bounds (world units) at zoom 1.0.
    base_width: f32,
    base_height: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            current_yaw: CAMERA_YAW_OFFSET,
            target_yaw: CAMERA_YAW_OFFSET,
            zoom: 1.0,
            base_width: 1.0,
            base_height: 1.0,
        }
    }
}

/// Marker for the single camera (used by `update_camera` and `apply_pretty`).
#[derive(Component)]
struct CityCamera;

/// Elevation of the camera above the horizon (radians). 45° gives the classic
/// three-quarter view where you see both block tops and building sides.
const CAMERA_ELEVATION: f32 = std::f32::consts::FRAC_PI_4;

/// The yaw is always offset 45° off the grid axes, so the view looks at block
/// corners (isometric-ish) rather than straight along a street.
const CAMERA_YAW_OFFSET: f32 = std::f32::consts::FRAC_PI_4;

/// Distance from the focus point to the camera. With an orthographic projection
/// this does not change apparent size — it only needs to sit inside the
/// near/far clip planes.
const CAMERA_DISTANCE: f32 = 200.0;

/// Rotation smoothing rate (higher = faster ease toward the target yaw).
const ROTATE_EASE: f32 = 8.0;

/// Pan speed in world units per second at zoom 1.0 (scales with zoom).
const PAN_SPEED: f32 = 60.0;

/// Multiplier applied to pan speed while Shift is held.
const SHIFT_PAN_MULTIPLIER: f32 = 3.0;

/// Multiplicative zoom amount per scroll-wheel notch.
const ZOOM_STEP: f32 = 0.1;

/// Zoom clamps: 1.0 is the default framing; these bound how far in and out the
/// camera can go (`base_width / zoom` is the visible width).
const ZOOM_MIN: f32 = 0.5;
const ZOOM_MAX: f32 = 4.0;

/// Default orthographic framing (world units) at zoom 1.0.
const CAMERA_BASE_WIDTH: f32 = 120.0;
const CAMERA_BASE_HEIGHT: f32 = 120.0;

/// Where the camera sits for a given focus point and yaw.
fn camera_position(focus: Vec3, yaw: f32) -> Vec3 {
    let horizontal = CAMERA_DISTANCE * CAMERA_ELEVATION.cos();
    let y = CAMERA_DISTANCE * CAMERA_ELEVATION.sin();
    focus + Vec3::new(horizontal * yaw.sin(), y, horizontal * yaw.cos())
}

/// Spawns the orthographic camera.
fn spawn_camera(mut commands: Commands, mut rig: ResMut<CameraRig>) {
    rig.base_width = CAMERA_BASE_WIDTH;
    rig.base_height = CAMERA_BASE_HEIGHT;

    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: rig.base_width,
                min_height: rig.base_height,
            },
            ..OrthographicProjection::default_3d()
        }),
        Msaa::Off,
        ProfileCameraMarker,
        CityCamera,
        Transform::from_translation(camera_position(Vec3::ZERO, CAMERA_YAW_OFFSET))
            .looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Per-frame camera input and smoothing: Q/E rotate (animated), WASD pans,
/// scroll zooms.
fn update_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mut scroll: MessageReader<MouseWheel>,
    time: Res<Time>,
    mut rig: ResMut<CameraRig>,
    mut transform: Single<&mut Transform, With<CityCamera>>,
    mut projection: Single<&mut Projection, With<CityCamera>>,
) {
    let dt = time.delta_secs();

    // Rotation target: Q/E step the yaw by 90°.
    if keys.just_pressed(KeyCode::KeyQ) {
        rig.target_yaw -= std::f32::consts::FRAC_PI_2; // counter-clockwise
    }
    if keys.just_pressed(KeyCode::KeyE) {
        rig.target_yaw += std::f32::consts::FRAC_PI_2; // clockwise
    }

    // Zoom: scroll up = zoom in.
    let scroll_y: f32 = scroll.read().map(|e| e.y).sum();
    if scroll_y != 0.0 {
        rig.zoom = (rig.zoom * (1.0 + scroll_y * ZOOM_STEP)).clamp(ZOOM_MIN, ZOOM_MAX);
    }

    // Ease the yaw toward its target (frame-rate independent).
    let blend = 1.0 - (-ROTATE_EASE * dt).exp();
    rig.current_yaw += (rig.target_yaw - rig.current_yaw) * blend;

    // Pan: move the focus along the camera's screen axes. `away` is the
    // horizontal "up on screen" direction (away from the camera); `right` is
    // the horizontal "right on screen" direction.
    let (sin, cos) = rig.current_yaw.sin_cos();
    let away = Vec3::new(-sin, 0.0, -cos);
    let right = Vec3::new(cos, 0.0, -sin);

    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        pan.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        pan.x -= 1.0;
    }
    let mut pan_speed = PAN_SPEED / rig.zoom;
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        pan_speed *= SHIFT_PAN_MULTIPLIER;
    }
    rig.focus += (right * pan.x + away * pan.y) * pan_speed * dt;

    // Apply the camera transform.
    transform.translation = camera_position(rig.focus, rig.current_yaw);
    transform.look_at(rig.focus, Vec3::Y);

    // Apply the projection framing from the current zoom.
    if let Projection::Orthographic(ref mut ortho) = **projection {
        ortho.scaling_mode = ScalingMode::AutoMin {
            min_width: rig.base_width / rig.zoom,
            min_height: rig.base_height / rig.zoom,
        };
    }
}

/// Marks the camera so `--pretty` can attach post-processing to it.
#[derive(Component, Default, Clone)]
struct ProfileCameraMarker;

/// Adds bloom + TAA to the camera when `--pretty`.
fn apply_pretty(
    mut commands: Commands,
    camera: Query<Entity, (With<ProfileCameraMarker>, With<Camera3d>)>,
) {
    for entity in &camera {
        commands.entity(entity).insert(Bloom::NATURAL);
        commands.entity(entity).insert(TemporalAntiAliasing::default());
    }
}
