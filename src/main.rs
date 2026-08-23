//! A minimalist, procedurally generated city.
//!
//! All geometry is generated at runtime from a small set of shared colored
//! primitives (cubes, cylinders, spheres), so the GPU only ever holds a handful
//! of meshes and Bevy batches every instance into a few draw calls. No glTF
//! assets, no HTTPS downloads, no network dependency. This is designed to run
//! smoothly on an integrated GPU (Mini Motorways-style visual style).

use argh::FromArgs;
use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    color::palettes::css::WHITE,
    feathers::{dark_theme::create_dark_theme, theme::UiTheme, FeathersPlugins},
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    post_process::bloom::Bloom,
    prelude::*,
    window::{PresentMode, WindowResolution},
    winit::WinitSettings,
};

use crate::generate_city::{setup_assets, spawn_city, MinimalAssets, SpawnConfig};
use crate::settings::{settings_ui, Settings};

mod diagnostics;
mod generate_city;
mod settings;

#[derive(FromArgs, Resource, Clone)]
/// Config
pub struct Args {
    /// seed
    #[argh(option, default = "42")]
    seed: u64,

    /// size
    #[argh(option, default = "12")]
    size: u32,

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

    /// disable the car simulation and cars entirely
    #[argh(switch)]
    no_cars: bool,

    /// disable buildings — keeps roads, ground, trees
    #[argh(switch)]
    no_buildings: bool,

    /// disable trees and fences — keeps ground, roads, buildings, cars
    #[argh(switch)]
    no_decorations: bool,

    /// disable everything except ground tiles and roads (minimal skeleton)
    #[argh(switch)]
    minimal: bool,

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

    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "minimal_city".into(),
                resolution: WindowResolution::new(args.width, args.height)
                    .with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                position: WindowPosition::Centered(MonitorSelection::Primary),
                ..default()
            }),
            ..default()
        }),
        FreeCameraPlugin,
        FeathersPlugins,
        WireframePlugin::default(),
    ))
    .insert_resource(args.clone())
    .insert_resource(ClearColor(Color::srgb(0.55, 0.8, 0.95)))
    .insert_resource(WinitSettings::continuous())
    .init_resource::<Settings>()
    .insert_resource(UiTheme(create_dark_theme()))
    .insert_resource(WireframeConfig {
        global: false,
        default_color: WHITE.into(),
        ..default()
    })
    .insert_resource(StaticTransformOptimizations::Enabled)
    .add_systems(
        Startup,
        (scene.spawn(), setup_assets, spawn_city_system).chain(),
    )
    .add_systems(Update, (simulate_cars, settings_ui.spawn()));

    if args.pretty {
        app.add_systems(Startup, apply_pretty);
    }
    if args.show_fps {
        app.add_systems(Startup, diagnostics::spawn_fps_overlay)
            .add_systems(Update, diagnostics::update_fps_overlay);
    }

    app.run();
}

fn scene() -> impl SceneList {
    bsn_list![camera()]
}

fn camera() -> impl Scene {
    bsn! {
        Camera3d
        template_value(Transform::from_xyz(15.0, 10.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y))
        FreeCamera
        Msaa::Off
        ProfileCameraMarker
    }
}

/// Marks the camera so `--low-graphics` can strip expensive post-processing.
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

/// Spawns the whole city once the shared meshes/materials exist.
fn spawn_city_system(
    mut commands: Commands,
    assets: Res<MinimalAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    args: Res<Args>,
) {
    spawn_city(
        &mut commands,
        &assets,
        &mut meshes,
        args.seed,
        args.size,
        SpawnConfig {
            no_cars: args.no_cars,
            no_buildings: args.no_buildings,
            no_decorations: args.no_decorations,
            minimal: args.minimal,
        },
    );
}

#[derive(Component)]
struct Road {
    start: Vec3,
    end: Vec3,
}

#[derive(Component)]
struct Car {
    offset: Vec3,
    distance_traveled: f32,
    dir: f32,
}

/// Naive traffic simulation: cars slide along their road segment.
fn simulate_cars(
    settings: Res<Settings>,
    args: Res<Args>,
    roads: Query<(&Road, &Transform, &Children), Without<Car>>,
    mut cars: Query<(&mut Car, &mut Transform), Without<Road>>,
    time: Res<Time>,
) {
    if !settings.simulate_cars || args.no_cars {
        return;
    }
    let speed = 1.5;

    for (road, _, children) in &roads {
        for child in children {
            let Ok((mut car, mut car_transform)) = cars.get_mut(*child) else {
                continue;
            };
            car.distance_traveled += speed * time.delta_secs();
            let road_len = (road.end - road.start).length();
            if car.distance_traveled > road_len {
                car.distance_traveled = 0.0;
            }
            let direction = (road.end - road.start).normalize() * car.dir;
            let progress = car.distance_traveled / road_len;
            car_transform.translation = (road.start + car.offset) + direction * road_len * progress;
        }
    }
}
