//! Procedural minimalist city: shared colored primitives instead of glTF assets.
//!
//! All geometry is generated at startup (cubes, cylinders, spheres) and reused
//! across every instance. Because every building/tree/car of a given type shares
//! the same mesh and material, Bevy batches them into a tiny number of draw calls
//! and the GPU never has to hold thousands of separate meshes — the exact problem
//! the detailed asset version hit (shared-memory thrash on integrated GPUs).

use bevy::prelude::*;
use noise::{NoiseFn, OpenSimplex};
use rand::{rngs::SmallRng, RngExt, SeedableRng};

/// The shared meshes and materials used to build the city. Created once at
/// startup; every spawned instance references these same handles.
#[derive(Resource)]
pub struct MinimalAssets {
    // Shared meshes
    pub cube: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    // Materials
    pub ground: Handle<StandardMaterial>,
    pub road: Handle<StandardMaterial>,
    pub trunk: Handle<StandardMaterial>,
    pub canopy: Handle<StandardMaterial>,
    pub fence: Handle<StandardMaterial>,
    pub buildings: Vec<Handle<StandardMaterial>>,
    pub cars: Vec<Handle<StandardMaterial>>,
}

/// Builds the shared meshes + material palette into a [`MinimalAssets`] resource.
pub fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let cylinder = meshes.add(Cylinder::new(0.5, 1.0));

    let mut mat = |r: u8, g: u8, b: u8| {
        materials.add(StandardMaterial::from_color(Color::srgb_u8(r, g, b)))
    };

    commands.insert_resource(MinimalAssets {
        cube,
        cylinder,
        ground: mat(97, 203, 139),   // grass green
        road: mat(50, 50, 55),       // dark asphalt
        trunk: mat(120, 80, 50),     // brown
        canopy: mat(46, 150, 80),    // tree green
        fence: mat(150, 150, 140),   // grey fence
        buildings: vec![
            mat(210, 95, 90),
            mat(240, 165, 85),
            mat(240, 210, 90),
            mat(95, 180, 210),
            mat(150, 130, 200),
            mat(240, 140, 190),
            mat(210, 210, 215),
        ],
        cars: vec![
            mat(60, 120, 220),
            mat(220, 60, 60),
            mat(240, 200, 60),
            mat(60, 190, 120),
            mat(255, 255, 255),
        ],
    });
}

/// Which parts of the city to spawn (used for performance bisection).
#[derive(Clone, Copy, Default)]
pub struct SpawnConfig {
    pub no_cars: bool,
    pub no_buildings: bool,
    pub no_decorations: bool,
    pub minimal: bool,
}

#[derive(Component)]
pub struct CityRoot;

/// Spawns a grid of minimalist city blocks.
///
/// Block pattern (each block is 5.5 x 4.0 units, same as the original):
/// X-------
/// | B B B
/// | B B B
/// X = crossroad, B = buildings
pub fn spawn_city(
    commands: &mut Commands,
    assets: &MinimalAssets,
    seed: u64,
    size: u32,
    config: SpawnConfig,
) {
    let mut rng = SmallRng::seed_from_u64(seed);
    let noise = OpenSimplex::new(rng.random());
    let noise_scale = 0.025;

    commands
        .spawn((CityRoot, Transform::default(), Visibility::default()))
        .with_children(|commands| {
            let half_size = size as i32 / 2;
            for x in -half_size..half_size {
                for z in -half_size..half_size {
                    let x = x as f32 * 5.5;
                    let z = z as f32 * 4.0;
                    let offset = Vec3::new(x, 0.0, z);

                    spawn_roads_and_cars(commands, assets, &mut rng, offset, config);

                    let density = noise.get([
                        offset.x as f64 * noise_scale,
                        offset.z as f64 * noise_scale,
                        0.0,
                    ]) * 0.5
                        + 0.5;

                    let forest = 0.45;
                    let low_density = 0.6;
                    let medium_density = 0.7;

                    // Ground tile
                    spawn_cube(
                        commands,
                        &assets.ground,
                        &assets.cube,
                        offset + Vec3::new(2.75, -0.5, 2.0),
                        Vec3::new(5.5, 0.4, 4.0),
                    );

                    if config.minimal {
                        continue;
                    }

                    if density < forest {
                        spawn_forest(commands, assets, offset, config);
                    } else if density < low_density {
                        spawn_low_density(commands, assets, &mut rng, offset, config);
                    } else if density < medium_density {
                        spawn_medium_density(commands, assets, &mut rng, offset, config);
                    } else {
                        spawn_high_density(commands, assets, &mut rng, offset, config);
                    }
                }
            }
        });
}

/// Spawns a single cube instance (translation is the cube's center).
fn spawn_cube(
    commands: &mut ChildSpawnerCommands,
    material: &Handle<StandardMaterial>,
    mesh: &Handle<Mesh>,
    translation: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(translation).with_scale(size),
    ));
}

/// Spawns a single cylinder instance (translation is the cylinder's center).
fn spawn_cylinder(
    commands: &mut ChildSpawnerCommands,
    material: &Handle<StandardMaterial>,
    mesh: &Handle<Mesh>,
    translation: Vec3,
    scale: Vec3,
) {
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(translation).with_scale(scale),
    ));
}

fn spawn_roads_and_cars<R: RngExt>(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    // Horizontal road strip
    spawn_cube(
        commands,
        &assets.road,
        &assets.cube,
        offset + Vec3::new(2.75, -0.35, 0.0),
        Vec3::new(5.5, 0.5, 1.0),
    );
    // Vertical road strip
    spawn_cube(
        commands,
        &assets.road,
        &assets.cube,
        offset + Vec3::new(0.0, -0.35, 2.0),
        Vec3::new(1.0, 0.5, 4.0),
    );

    if config.no_cars || config.minimal {
        return;
    }

    // Cars along the horizontal road
    for i in 0..6 {
        let car_pos = Vec3::new(0.75 + i as f32 * 0.8, 0.0, 0.0);
        if rng.random::<f32>() < 0.25 {
            spawn_cube(
                commands,
                &assets.cars[rng.random_range(0..assets.cars.len())],
                &assets.cube,
                offset + car_pos + Vec3::new(0.0, 0.35, -0.25),
                Vec3::new(0.5, 0.3, 0.3),
            );
        }
    }
}

fn spawn_low_density<R: RngExt>(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    if !config.no_buildings {
        for x in 1..=2 {
            let height = 1.0 + rng.random::<f32>() * 1.5;
            spawn_building(commands, assets, rng, offset + Vec3::new(x as f32 * 1.8, 0.0, 1.5), height);
            spawn_building(commands, assets, rng, offset + Vec3::new(x as f32 * 1.8, 0.0, 3.0), height);
        }
    }
    if !config.no_decorations {
        for i in 0..=6 {
            spawn_tree(commands, assets, offset + Vec3::new(0.75, 0.0, 0.75 + i as f32 * 0.4));
            spawn_tree(commands, assets, offset + Vec3::new(4.75, 0.0, 0.75 + i as f32 * 0.4));
        }
        for i in 0..=4 {
            spawn_cylinder(
                commands,
                &assets.fence,
                &assets.cylinder,
                offset + Vec3::new(2.75, 0.15, 0.75 + i as f32 * 0.6),
                Vec3::new(0.08, 0.3, 0.08),
            );
        }
    }
}

fn spawn_medium_density<R: RngExt>(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    if !config.no_buildings {
        for x in 1..=5 {
            let height = 1.5 + rng.random::<f32>() * 2.5;
            spawn_building(commands, assets, rng, offset + Vec3::new(x as f32 * 0.9, 0.0, 1.25), height);
            spawn_building(commands, assets, rng, offset + Vec3::new(x as f32 * 0.9, 0.0, 3.0), height * 0.7);
        }
    }
    if !config.no_decorations {
        for x in 1..=4 {
            spawn_tree(commands, assets, offset + Vec3::new(x as f32 * 0.9, 0.0, 2.0));
        }
    }
}

fn spawn_high_density<R: RngExt>(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    if config.no_buildings {
        return;
    }
    for x in 0..3 {
        let height = 2.0 + rng.random::<f32>() * 3.0;
        spawn_building(commands, assets, rng, offset + Vec3::new(1.25 + x as f32 * 1.5, 0.0, 1.25), height);
        spawn_building(commands, assets, rng, offset + Vec3::new(1.25 + x as f32 * 1.5, 0.0, 3.0), height * 0.8);
    }
}

fn spawn_forest(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    offset: Vec3,
    config: SpawnConfig,
) {
    if config.no_decorations {
        return;
    }
    for x in 0..=12 {
        for z in 0..=8 {
            if (x + z) % 2 == 0 {
                continue;
            }
            let pos = offset
                + Vec3::new(x as f32, 0.0, z as f32) * Vec3::new(0.325, 0.0, 0.3)
                + Vec3::new(0.75, 0.0, 0.85);
            spawn_tree(commands, assets, pos);
        }
    }
}

fn spawn_building<R: RngExt>(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    rng: &mut R,
    translation: Vec3,
    height: f32,
) {
    spawn_cube(
        commands,
        &assets.buildings[rng.random_range(0..assets.buildings.len())],
        &assets.cube,
        translation + Vec3::Y * (height / 2.0),
        Vec3::new(0.7, height, 0.7),
    );
}

fn spawn_tree(
    commands: &mut ChildSpawnerCommands,
    assets: &MinimalAssets,
    translation: Vec3,
) {
    // trunk
    spawn_cylinder(
        commands,
        &assets.trunk,
        &assets.cylinder,
        translation + Vec3::new(0.0, 0.4, 0.0),
        Vec3::new(0.08, 0.8, 0.08),
    );
    // canopy
    spawn_cube(
        commands,
        &assets.canopy,
        &assets.cube,
        translation + Vec3::new(0.0, 1.1, 0.0),
        Vec3::new(0.5, 0.5, 0.5),
    );
}
