//! Road-placement mechanic: a flat ground plane and straight road segments the
//! player draws with the mouse, rendered as one tessellated mesh with smooth
//! (round) joins and caps.
//!
//! The road network is a set of polylines ("chains"). Committed chains are
//! stroked with lyon (round joins + round caps) into a single flat mesh, which
//! gives consistent road width at any turn angle and clean T-junctions and
//! crossings.
//!
//! Interaction model:
//! - A translucent gray circle (diameter = road width) with an opaque ring
//!   stroke marks the cursor, snapping onto an existing road when close.
//! - Left-click starts a chain (splitting an existing chain if it lands on its
//!   interior, forming a T-junction).
//! - Each further left-click commits a segment and chains a new one from its
//!   end, so consecutive segments can be placed quickly.
//! - Right-click finalizes the chain and returns to neutral.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    window::PrimaryWindow,
};
use lyon_path::Path;
use lyon_tessellation::{
    math::{point, Point},
    BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
};

/// Road width in world units; the cursor circle has this diameter.
const ROAD_WIDTH: f32 = 1.0;

/// Height of the flat road mesh above the ground (avoids z-fighting).
const ROAD_Y: f32 = 0.01;

/// Height of the cursor fill circle above the ground.
const CURSOR_Y: f32 = 0.02;

/// Height of the cursor stroke ring above the fill.
const CURSOR_STROKE_Y: f32 = 0.03;

/// Clicking within this distance of an existing road snaps the cursor to it.
const ROAD_SNAP_DISTANCE: f32 = ROAD_WIDTH * 0.6;

/// Tolerance for treating two ground points as coincident (join detection).
const JOIN_EPSILON: f32 = 1e-3;

/// Side length of the square ground plane, in world units.
const GROUND_SIZE: f32 = 200.0;

/// Ground color `#2e3d2a`.
const GROUND_RGB: (u8, u8, u8) = (0x2e, 0x3d, 0x2a);

/// Gray used for roads (opaque) and the cursor fill (translucent).
const ROAD_RGB: (u8, u8, u8) = (120, 122, 128);

/// Opaque ring color for the cursor stroke (white, to stay visible over roads).
const CURSOR_STROKE_RGB: (u8, u8, u8) = (255, 255, 255);

/// Thickness of the cursor ring stroke, in world units.
const CURSOR_STROKE_WIDTH: f32 = 0.06;

/// Shared meshes/materials for the road mechanic.
#[derive(Resource)]
pub(crate) struct RoadAssets {
    road_preview: Handle<StandardMaterial>,
    /// Flat unit rectangle (1.0 × `ROAD_WIDTH`) laid flat and scaled per segment
    /// to draw the translucent preview.
    preview_mesh: Handle<Mesh>,
}

/// The committed road network: a set of polylines (`(x, z)` points).
#[derive(Resource, Default)]
pub(crate) struct RoadNetwork {
    chains: Vec<Vec<Vec2>>,
}

/// Placement interaction state.
#[derive(Resource, Default)]
pub(crate) enum Placement {
    #[default]
    Neutral,
    /// A chain is being drawn: `points` are its committed vertices so far (one
    /// per confirmed segment), and `preview` is the translucent preview entity.
    Placing { points: Vec<Vec2>, preview: Entity },
}

/// The translucent circle + ring marking the cursor's ground position.
#[derive(Component)]
pub(crate) struct CursorMarker;

/// The translucent preview road shown while placing.
#[derive(Component)]
pub(crate) struct PreviewRoad;

/// The single merged road mesh entity.
#[derive(Component)]
pub(crate) struct RoadMeshMarker;

/// Builds an unlit [`StandardMaterial`] (translucent when `alpha < 1.0`), with
/// backface culling off so flat tessellated geometry renders from above.
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
        cull_mode: None,
        ..default()
    })
}

/// Spawns the ground plane, the cursor, the empty road mesh, and the assets.
pub(crate) fn setup_roads(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let ground = solid_material(&mut materials, GROUND_RGB, 1.0);
    let road_opaque = solid_material(&mut materials, ROAD_RGB, 1.0);
    let road_preview = solid_material(&mut materials, ROAD_RGB, 0.5);
    let cursor_fill = solid_material(&mut materials, ROAD_RGB, 0.4);
    let cursor_stroke = solid_material(&mut materials, CURSOR_STROKE_RGB, 1.0);

    let preview_mesh = meshes.add(Mesh::from(Rectangle::new(1.0, ROAD_WIDTH)));

    commands.insert_resource(RoadAssets {
        road_preview: road_preview.clone(),
        preview_mesh: preview_mesh.clone(),
    });
    commands.insert_resource(RoadNetwork::default());
    commands.insert_resource(Placement::default());

    // Ground plane (a large flat quad in the XZ plane at y = 0).
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::default()))),
        MeshMaterial3d(ground),
        Transform::from_scale(Vec3::new(GROUND_SIZE, 1.0, GROUND_SIZE)),
    ));

    // The merged road mesh (starts empty; rebuilt as roads are placed).
    commands.spawn((
        Mesh3d(meshes.add(tessellate_roads(&RoadNetwork::default(), &[]))),
        MeshMaterial3d(road_opaque),
        RoadMeshMarker,
        Transform::default(),
    ));

    // Cursor: a flat circle (translucent fill) plus an opaque ring stroke,
    // parented so both move together. The marker holds the position.
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let radius = ROAD_WIDTH / 2.0;
    commands
        .spawn((
            CursorMarker,
            Transform::from_xyz(0.0, 0.0, 0.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(Mesh::from(Circle::new(radius)))),
                MeshMaterial3d(cursor_fill),
                Transform::from_xyz(0.0, CURSOR_Y, 0.0).with_rotation(flat),
            ));
            parent.spawn((
                Mesh3d(meshes.add(Mesh::from(Annulus::new(
                    radius - CURSOR_STROKE_WIDTH,
                    radius,
                )))),
                MeshMaterial3d(cursor_stroke),
                Transform::from_xyz(0.0, CURSOR_STROKE_Y, 0.0).with_rotation(flat),
            ));
        });
}

/// Per-frame road placement: moves the cursor, handles clicks, and updates /
/// commits / finalizes chains.
///
/// Runs in `PostUpdate` after transform propagation so the camera's
/// `GlobalTransform` reflects this frame's pan/rotation.
pub(crate) fn road_placement_system(
    mut commands: Commands,
    assets: Res<RoadAssets>,
    mut placement: ResMut<Placement>,
    mut network: ResMut<RoadNetwork>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut cursor_q: Query<&mut Transform, (With<CursorMarker>, Without<PreviewRoad>)>,
    mut preview_q: Query<&mut Transform, (With<PreviewRoad>, Without<CursorMarker>)>,
    mut road_mesh_q: Query<&mut Mesh3d, (With<RoadMeshMarker>, Without<CursorMarker>)>,
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

    // Snap the cursor onto an existing road when close, and position the cursor
    // circle at the snapped point.
    let snapped = snap_to_roads(ground, &network);
    if let Ok(mut tf) = cursor_q.single_mut() {
        tf.translation = Vec3::new(snapped.x, 0.0, snapped.y);
    }

    // Take ownership of the state, compute the next state, then put it back.
    let state = std::mem::replace(&mut *placement, Placement::Neutral);

    let new_state = match state {
        Placement::Neutral => {
            if buttons.just_pressed(MouseButton::Left) {
                // Start a new chain. If the start lands on an existing road's
                // interior, split that chain so a T-junction node is formed.
                split_chain_at(&mut network, snapped);
                let preview = spawn_preview(&mut commands, &assets, snapped, snapped);
                Placement::Placing {
                    points: vec![snapped],
                    preview,
                }
            } else {
                Placement::Neutral
            }
        }
        Placement::Placing {
            mut points,
            preview,
        } => {
            // Draw the preview from the last committed point to the cursor.
            if let Some(last) = points.last().copied() {
                if let Ok(mut tf) = preview_q.get_mut(preview) {
                    set_road_transform(&mut tf, last, snapped);
                }
            }

            if buttons.just_pressed(MouseButton::Left) {
                // Commit this segment: append the snapped point and rebuild the
                // merged mesh so the committed chain renders opaque.
                if points
                    .last()
                    .copied()
                    .is_none_or(|p| p.distance(snapped) > ROAD_WIDTH * 0.2)
                {
                    points.push(snapped);
                }
                rebuild_road_mesh(&network, &points, &mut meshes, &mut road_mesh_q);
                Placement::Placing { points, preview }
            } else if buttons.just_pressed(MouseButton::Right) {
                // Finalize: move the chain into the network and leave placing.
                if points.len() >= 2 {
                    network.chains.push(points);
                }
                commands.entity(preview).despawn();
                rebuild_road_mesh(&network, &[], &mut meshes, &mut road_mesh_q);
                Placement::Neutral
            } else {
                Placement::Placing { points, preview }
            }
        }
    };

    *placement = new_state;
}

/// Rebuilds the merged road mesh from the finalized chains plus the active chain.
fn rebuild_road_mesh(
    network: &RoadNetwork,
    active: &[Vec2],
    meshes: &mut Assets<Mesh>,
    road_mesh_q: &mut Query<&mut Mesh3d, (With<RoadMeshMarker>, Without<CursorMarker>)>,
) {
    let mesh = tessellate_roads(network, active);
    let handle = meshes.add(mesh);
    if let Ok(mut mesh3d) = road_mesh_q.single_mut() {
        *mesh3d = Mesh3d(handle);
    }
}

/// Strokes every chain into one flat triangle mesh (round joins + caps).
fn tessellate_roads(network: &RoadNetwork, active: &[Vec2]) -> Mesh {
    let mut geometry: VertexBuffers<Point, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    let options = StrokeOptions::tolerance(0.05)
        .with_line_width(ROAD_WIDTH)
        .with_line_join(LineJoin::Round)
        .with_line_cap(LineCap::Round);

    let mut chains: Vec<&[Vec2]> = network.chains.iter().map(|c| c.as_slice()).collect();
    if active.len() >= 2 {
        chains.push(active);
    }

    for chain in chains {
        if chain.len() < 2 {
            continue;
        }
        let mut builder = Path::builder();
        builder.begin(point(chain[0].x, chain[0].y));
        for p in &chain[1..] {
            builder.line_to(point(p.x, p.y));
        }
        builder.end(false);
        let path = builder.build();
        let _ = tessellator.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex| v.position()),
        );
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let positions: Vec<[f32; 3]> = geometry
        .vertices
        .iter()
        .map(|p| [p.x, ROAD_Y, p.y])
        .collect();
    let normals: Vec<[f32; 3]> = positions.iter().map(|_| [0.0, 1.0, 0.0]).collect();
    let uvs: Vec<[f32; 2]> = geometry.vertices.iter().map(|p| [p.x, p.y]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(geometry.indices));
    mesh
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
fn snap_to_roads(point: Vec2, network: &RoadNetwork) -> Vec2 {
    let mut closest = point;
    let mut best = ROAD_SNAP_DISTANCE;
    for chain in &network.chains {
        for i in 0..chain.len().saturating_sub(1) {
            let proj = closest_point_on_segment(point, chain[i], chain[i + 1]);
            let d = point.distance(proj);
            if d < best {
                best = d;
                closest = proj;
            }
        }
    }
    closest
}

/// Splits the chain containing `point` (if it lies on a segment's interior) at
/// that point, so a new road can join there as a T-junction.
fn split_chain_at(network: &mut RoadNetwork, point: Vec2) {
    for chain in &mut network.chains {
        for i in 0..chain.len().saturating_sub(1) {
            let (a, b) = (chain[i], chain[i + 1]);
            let t = closest_point_param(point, a, b);
            let proj = a + (b - a) * t;
            if point.distance(proj) <= JOIN_EPSILON {
                // On this segment. Split only if it's the interior (not an endpoint).
                if t > JOIN_EPSILON && t < 1.0 - JOIN_EPSILON {
                    chain.insert(i + 1, point);
                }
                return;
            }
        }
    }
}

/// Closest point on segment `a..b` to `p`.
fn closest_point_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let t = closest_point_param(p, a, b);
    a + (b - a) * t
}

/// Parameter `t` (in `[0, 1]`) of the closest point on segment `a..b` to `p`.
fn closest_point_param(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-8 {
        return 0.0;
    }
    ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0)
}

/// Orients `tf` as a flat road strip running from `start` to `end`.
fn set_road_transform(tf: &mut Transform, start: Vec2, end: Vec2) {
    let dir = end - start;
    let length = dir.length();
    let midpoint = (start + end) * 0.5;
    // Align the strip's local +X with the world (dx, dz) direction, after laying
    // it flat (local +Z normal -> +Y up).
    let yaw = (-dir.y).atan2(dir.x);
    *tf = Transform::from_translation(Vec3::new(midpoint.x, ROAD_Y, midpoint.y))
        .with_rotation(
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        )
        .with_scale(Vec3::new(length, 1.0, 1.0));
}

/// Spawns the translucent preview road and returns its entity.
fn spawn_preview(commands: &mut Commands, assets: &RoadAssets, start: Vec2, end: Vec2) -> Entity {
    let mut tf = Transform::default();
    set_road_transform(&mut tf, start, end);
    commands
        .spawn((
            Mesh3d(assets.preview_mesh.clone()),
            MeshMaterial3d(assets.road_preview.clone()),
            PreviewRoad,
            tf,
        ))
        .id()
}
