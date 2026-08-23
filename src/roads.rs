//! Road-placement mechanic: a flat ground plane and straight road segments the
//! player draws with the mouse.
//!
//! Interaction model:
//! - A translucent gray sphere (diameter = road width) marks the cursor's
//!   position on the ground.
//! - Left-click picks the start point (snapping onto an existing road when the
//!   cursor is close) and enters the placing state.
//! - In the placing state a translucent road is drawn from the start point to
//!   the cursor; a second left-click commits it as an opaque road.
//! - Right-click in the placing state cancels back to neutral without adding
//!   anything.

use bevy::{prelude::*, window::PrimaryWindow};

/// Road width in world units; the cursor sphere has this diameter.
const ROAD_WIDTH: f32 = 1.0;

/// Height of a road slab (a thin box lying flat on the ground).
const ROAD_HEIGHT: f32 = 0.05;

/// Clicking within this distance of an existing road snaps the start point to it.
const ROAD_SNAP_DISTANCE: f32 = ROAD_WIDTH * 0.6;

/// Side length of the square ground plane, in world units.
const GROUND_SIZE: f32 = 200.0;

/// Ground color `#2e3d2a`.
const GROUND_RGB: (u8, u8, u8) = (0x2e, 0x3d, 0x2a);

/// Gray used for roads (opaque) and for the preview/cursor (translucent).
const ROAD_RGB: (u8, u8, u8) = (120, 122, 128);

/// Shared meshes/materials for the road mechanic.
#[derive(Resource)]
pub(crate) struct RoadAssets {
    /// Unit-length road slab (1.0 along local X, `ROAD_WIDTH` wide). Every
    /// segment scales it along X to its own length, so one mesh serves all.
    road_mesh: Handle<Mesh>,
    road_opaque: Handle<StandardMaterial>,
    road_preview: Handle<StandardMaterial>,
}

/// Placement interaction state.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) enum Placement {
    #[default]
    Neutral,
    /// A start point is chosen; a preview follows the cursor until confirmed.
    Placing { start: Vec2, preview: Entity },
}

/// A committed, permanent straight road segment. Endpoints live on the y=0
/// plane and are stored as `(x, z)` pairs.
#[derive(Component)]
pub(crate) struct RoadSegment {
    start: Vec2,
    end: Vec2,
}

/// The translucent sphere marking the cursor's ground position.
#[derive(Component)]
pub(crate) struct CursorMarker;

/// The translucent preview road shown while placing.
#[derive(Component)]
pub(crate) struct PreviewRoad;

/// Builds an unlit [`StandardMaterial`] (translucent when `alpha < 1.0`).
fn solid_material(
    materials: &mut Assets<StandardMaterial>,
    rgb: (u8, u8, u8),
    alpha: f32,
) -> Handle<StandardMaterial> {
    let (r, g, b) = rgb;
    materials.add(StandardMaterial {
        base_color: Color::srgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            alpha,
        ),
        alpha_mode: if alpha < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        unlit: true,
        ..default()
    })
}

/// Spawns the ground plane, the cursor sphere, and the shared road assets.
pub(crate) fn setup_roads(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let ground = solid_material(&mut materials, GROUND_RGB, 1.0);
    let road_opaque = solid_material(&mut materials, ROAD_RGB, 1.0);
    let road_preview = solid_material(&mut materials, ROAD_RGB, 0.5);
    let cursor = solid_material(&mut materials, ROAD_RGB, 0.5);

    let road_mesh = meshes.add(Cuboid::new(1.0, ROAD_HEIGHT, ROAD_WIDTH));

    commands.insert_resource(RoadAssets {
        road_mesh: road_mesh.clone(),
        road_opaque: road_opaque.clone(),
        road_preview: road_preview.clone(),
    });
    commands.insert_resource(Placement::default());

    // Ground plane (a large flat quad in the XZ plane at y = 0).
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::default()))),
        MeshMaterial3d(ground),
        Transform::from_scale(Vec3::new(GROUND_SIZE, 1.0, GROUND_SIZE)),
    ));

    // Cursor sphere (diameter = road width), sitting on the ground.
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Sphere::new(ROAD_WIDTH / 2.0)))),
        MeshMaterial3d(cursor),
        CursorMarker,
        Transform::from_xyz(0.0, ROAD_WIDTH / 2.0, 0.0),
    ));
}

/// Per-frame road placement: moves the cursor sphere, handles clicks, and
/// updates / commits / cancels the preview road.
///
/// Runs in `PostUpdate` after transform propagation so the camera's
/// `GlobalTransform` reflects this frame's pan/rotation.
pub(crate) fn road_placement_system(
    mut commands: Commands,
    assets: Res<RoadAssets>,
    mut placement: ResMut<Placement>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut cursor_q: Query<&mut Transform, (With<CursorMarker>, Without<PreviewRoad>)>,
    mut preview_q: Query<&mut Transform, (With<PreviewRoad>, Without<CursorMarker>)>,
    roads_q: Query<&RoadSegment>,
) {
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };
    let Ok(window) = window_q.single() else {
        return;
    };
    let Some(screen_pos) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, screen_pos) else {
        return;
    };
    let Some(ground) = ray_on_ground(&ray) else {
        return;
    };

    // The cursor sphere follows the mouse.
    if let Ok(mut tf) = cursor_q.single_mut() {
        tf.translation = Vec3::new(ground.x, ROAD_WIDTH / 2.0, ground.y);
    }

    let state = *placement;
    match state {
        Placement::Neutral => {
            if buttons.just_pressed(MouseButton::Left) {
                let start = snap_to_roads(ground, &roads_q);
                let preview = spawn_preview(&mut commands, &assets, start, ground);
                *placement = Placement::Placing { start, preview };
            }
        }
        Placement::Placing { start, preview } => {
            if let Ok(mut tf) = preview_q.get_mut(preview) {
                set_road_transform(&mut tf, start, ground);
            }

            if buttons.just_pressed(MouseButton::Left) {
                spawn_road(&mut commands, &assets, start, ground);
                commands.entity(preview).despawn();
                *placement = Placement::Neutral;
            } else if buttons.just_pressed(MouseButton::Right) {
                commands.entity(preview).despawn();
                *placement = Placement::Neutral;
            }
        }
    }
}

/// Intersects a world-space ray with the y=0 plane, returning the `(x, z)` point.
fn ray_on_ground(ray: &Ray3d) -> Option<Vec2> {
    let origin = ray.origin;
    let dir = ray.direction.as_vec3();
    if dir.y.abs() < 1e-6 {
        return None; // parallel to the ground
    }
    let t = -origin.y / dir.y;
    if t < 0.0 {
        return None; // intersection behind the camera
    }
    Some(Vec2::new(origin.x + dir.x * t, origin.z + dir.z * t))
}

/// Snaps `point` to the closest point on an existing road, if within snap range.
fn snap_to_roads(point: Vec2, roads: &Query<&RoadSegment>) -> Vec2 {
    let mut closest = point;
    let mut best = ROAD_SNAP_DISTANCE;
    for road in roads {
        let proj = closest_point_on_segment(point, road.start, road.end);
        let d = point.distance(proj);
        if d < best {
            best = d;
            closest = proj;
        }
    }
    closest
}

/// Closest point on segment `a..b` to `p`.
fn closest_point_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-8 {
        return a;
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// Orients `tf` as a road slab running from `start` to `end`.
fn set_road_transform(tf: &mut Transform, start: Vec2, end: Vec2) {
    let dir = end - start;
    let length = dir.length();
    let midpoint = (start + end) * 0.5;
    // Align the slab's local +X with the world (dx, dz) direction.
    let yaw = (-dir.y).atan2(dir.x);
    *tf = Transform::from_xyz(midpoint.x, ROAD_HEIGHT / 2.0, midpoint.y)
        .with_rotation(Quat::from_rotation_y(yaw))
        .with_scale(Vec3::new(length, 1.0, 1.0));
}

/// Spawns the translucent preview road and returns its entity.
fn spawn_preview(commands: &mut Commands, assets: &RoadAssets, start: Vec2, end: Vec2) -> Entity {
    let mut tf = Transform::default();
    set_road_transform(&mut tf, start, end);
    commands
        .spawn((
            Mesh3d(assets.road_mesh.clone()),
            MeshMaterial3d(assets.road_preview.clone()),
            PreviewRoad,
            tf,
        ))
        .id()
}

/// Commits a permanent road segment, skipping degenerate zero-length segments.
fn spawn_road(commands: &mut Commands, assets: &RoadAssets, start: Vec2, end: Vec2) {
    if start.distance(end) < ROAD_WIDTH * 0.2 {
        return;
    }
    let mut tf = Transform::default();
    set_road_transform(&mut tf, start, end);
    commands.spawn((
        Mesh3d(assets.road_mesh.clone()),
        MeshMaterial3d(assets.road_opaque.clone()),
        RoadSegment { start, end },
        tf,
    ));
}
