//! Procedural minimalist city: merged meshes instead of per-instance entities.
//!
//! The whole city is built from shared colored primitives. Crucially, all
//! geometry is *merged* into one mesh per material color at spawn time, so the
//! entire city is ~16 entities and ~16 draw calls — not thousands of separate
//! boxes. This is what makes a Mini Motorways-style scene fast on an integrated
//! GPU: near-zero entity/culling/batching overhead and minimal draw calls.

use bevy::prelude::*;
use noise::{NoiseFn, OpenSimplex};
use rand::{rngs::SmallRng, RngExt, SeedableRng};

/// The shared materials used to build the city. Created once at startup.
/// (Meshes are merged on demand from a unit cube, so no per-type mesh handles
/// are needed here.)
#[derive(Resource)]
pub struct MinimalAssets {
    pub ground: Handle<StandardMaterial>,
    pub road: Handle<StandardMaterial>,
    pub canopy: Handle<StandardMaterial>,
    pub fence: Handle<StandardMaterial>,
    pub buildings: Vec<Handle<StandardMaterial>>,
    pub cars: Vec<Handle<StandardMaterial>>,
}

/// Builds the shared material palette into a [`MinimalAssets`] resource.
pub fn setup_assets(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Unlit flat-color materials: no lighting/shadow math per fragment, which
    // is the Mini Motorways look AND the biggest GPU win on integrated graphics.
    let mut mat = |r: u8, g: u8, b: u8| {
        materials.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            unlit: true,
            ..default()
        })
    };

    commands.insert_resource(MinimalAssets {
        ground: mat(97, 203, 139), // grass green
        road: mat(50, 50, 55),     // dark asphalt
        canopy: mat(46, 150, 80),  // tree green
        fence: mat(150, 150, 140), // grey fence
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

/// One box instance pending merge: a material and a transform (translation +
/// scale baked into the vertex data at merge time).
struct Instance {
    material: Handle<StandardMaterial>,
    transform: Transform,
}

/// Spawns a grid of minimalist city blocks, merging all geometry into one mesh
/// per material color.
pub fn spawn_city(
    commands: &mut Commands,
    assets: &MinimalAssets,
    meshes: &mut Assets<Mesh>,
    seed: u64,
    size: u32,
    config: SpawnConfig,
) {
    let mut rng = SmallRng::seed_from_u64(seed);
    let noise = OpenSimplex::new(rng.random());
    let noise_scale = 0.025;

    let mut instances: Vec<Instance> = Vec::new();

    let half_size = size as i32 / 2;
    for x in -half_size..half_size {
        for z in -half_size..half_size {
            let x = x as f32 * 5.5;
            let z = z as f32 * 4.0;
            let offset = Vec3::new(x, 0.0, z);

            push_roads_and_cars(&mut instances, assets, &mut rng, offset, config);

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
            push_cube(
                &mut instances,
                &assets.ground,
                offset + Vec3::new(2.75, -0.5, 2.0),
                Vec3::new(5.5, 0.4, 4.0),
            );

            if config.minimal {
                continue;
            }

            if density < forest {
                push_forest(&mut instances, assets, offset, config);
            } else if density < low_density {
                push_low_density(&mut instances, assets, &mut rng, offset, config);
            } else if density < medium_density {
                push_medium_density(&mut instances, assets, &mut rng, offset, config);
            } else {
                push_high_density(&mut instances, assets, &mut rng, offset, config);
            }
        }
    }

    // Merge all instances into one mesh per material, then spawn one entity per
    // material. This collapses thousands of boxes into ~16 merged meshes.
    let mut groups: Vec<(Handle<StandardMaterial>, Vec<Transform>)> = Vec::new();
    for inst in instances {
        if let Some((_, transforms)) = groups
            .iter_mut()
            .find(|(mat, _)| *mat == inst.material)
        {
            transforms.push(inst.transform);
        } else {
            groups.push((inst.material.clone(), vec![inst.transform]));
        }
    }

    commands.spawn((
        CityRoot,
        Transform::default(),
        Visibility::default(),
    )).with_children(|commands| {
        for (material, transforms) in &groups {
            let mut merged: Option<Mesh> = None;
            for transform in transforms {
                let cube = Mesh::from(Cuboid::new(1.0, 1.0, 1.0)).transformed_by(*transform);
                match &mut merged {
                    Some(m) => {
                        let _ = m.merge(&cube);
                    }
                    None => merged = Some(cube),
                }
            }
            if let Some(mesh) = merged {
                let handle = meshes.add(mesh);
                commands.spawn((
                    Mesh3d(handle),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::default(),
                ));
            }
        }
    });
}

/// Records a cube instance for later merge (translation is the cube's center).
fn push_cube(
    instances: &mut Vec<Instance>,
    material: &Handle<StandardMaterial>,
    translation: Vec3,
    size: Vec3,
) {
    instances.push(Instance {
        material: material.clone(),
        transform: Transform::from_translation(translation).with_scale(size),
    });
}

fn push_roads_and_cars<R: RngExt>(
    instances: &mut Vec<Instance>,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    // Horizontal road strip
    push_cube(
        instances,
        &assets.road,
        offset + Vec3::new(2.75, -0.35, 0.0),
        Vec3::new(5.5, 0.5, 1.0),
    );
    // Vertical road strip
    push_cube(
        instances,
        &assets.road,
        offset + Vec3::new(0.0, -0.35, 2.0),
        Vec3::new(1.0, 0.5, 4.0),
    );

    if config.no_cars || config.minimal {
        return;
    }

    for i in 0..6 {
        let car_pos = Vec3::new(0.75 + i as f32 * 0.8, 0.0, 0.0);
        if rng.random::<f32>() < 0.25 {
            push_cube(
                instances,
                &assets.cars[rng.random_range(0..assets.cars.len())],
                offset + car_pos + Vec3::new(0.0, 0.35, -0.25),
                Vec3::new(0.5, 0.3, 0.3),
            );
        }
    }
}

fn push_low_density<R: RngExt>(
    instances: &mut Vec<Instance>,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    if !config.no_buildings {
        for x in 1..=2 {
            let height = 1.0 + rng.random::<f32>() * 1.5;
            push_building(instances, assets, rng, offset + Vec3::new(x as f32 * 1.8, 0.0, 1.5), height);
            push_building(instances, assets, rng, offset + Vec3::new(x as f32 * 1.8, 0.0, 3.0), height);
        }
    }
    if !config.no_decorations {
        for i in 0..=6 {
            push_tree(instances, assets, offset + Vec3::new(0.75, 0.0, 0.75 + i as f32 * 0.4));
            push_tree(instances, assets, offset + Vec3::new(4.75, 0.0, 0.75 + i as f32 * 0.4));
        }
        for i in 0..=4 {
            push_cube(
                instances,
                &assets.fence,
                offset + Vec3::new(2.75, 0.15, 0.75 + i as f32 * 0.6),
                Vec3::new(0.08, 0.3, 0.08),
            );
        }
    }
}

fn push_medium_density<R: RngExt>(
    instances: &mut Vec<Instance>,
    assets: &MinimalAssets,
    rng: &mut R,
    offset: Vec3,
    config: SpawnConfig,
) {
    if !config.no_buildings {
        for x in 1..=5 {
            let height = 1.5 + rng.random::<f32>() * 2.5;
            push_building(instances, assets, rng, offset + Vec3::new(x as f32 * 0.9, 0.0, 1.25), height);
            push_building(instances, assets, rng, offset + Vec3::new(x as f32 * 0.9, 0.0, 3.0), height * 0.7);
        }
    }
    if !config.no_decorations {
        for x in 1..=4 {
            push_tree(instances, assets, offset + Vec3::new(x as f32 * 0.9, 0.0, 2.0));
        }
    }
}

fn push_high_density<R: RngExt>(
    instances: &mut Vec<Instance>,
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
        push_building(instances, assets, rng, offset + Vec3::new(1.25 + x as f32 * 1.5, 0.0, 1.25), height);
        push_building(instances, assets, rng, offset + Vec3::new(1.25 + x as f32 * 1.5, 0.0, 3.0), height * 0.8);
    }
}

fn push_forest(
    instances: &mut Vec<Instance>,
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
            push_tree(instances, assets, pos);
        }
    }
}

fn push_building<R: RngExt>(
    instances: &mut Vec<Instance>,
    assets: &MinimalAssets,
    rng: &mut R,
    translation: Vec3,
    height: f32,
) {
    push_cube(
        instances,
        &assets.buildings[rng.random_range(0..assets.buildings.len())],
        translation + Vec3::Y * (height / 2.0),
        Vec3::new(0.7, height, 0.7),
    );
}

fn push_tree(
    instances: &mut Vec<Instance>,
    assets: &MinimalAssets,
    translation: Vec3,
) {
    // A single flat-green box is the minimalist tree.
    push_cube(
        instances,
        &assets.canopy,
        translation + Vec3::new(0.0, 0.5, 0.0),
        Vec3::new(0.4, 1.0, 0.4),
    );
}
