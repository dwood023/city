//! Road-placement mechanic: a flat ground plane and road segments the player
//! draws with the mouse, rendered as one tessellated mesh with smooth (round)
//! joins and caps.
//!
//! The road network is a set of polylines ("chains"). Committed chains are
//! stroked with lyon (round joins + round caps) into a single flat mesh, which
//! gives consistent road width at any turn angle and clean T-junctions and
//! crossings. A quadratic-Bezier curve is sampled into a polyline before being
//! added, so curves and straight segments share one rendering path.
//!
//! Two placement modes (toggle with `C`):
//! - **Straight** (default): left-click starts a chain; each further left-click
//!   commits a segment and chains; right-click finalizes.
//! - **Curve**: left-click sets the start, a second left-click sets the control
//!   point, and a third left-click confirms the end (a quadratic Bezier).

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

/// Step size for angle snapping (90° = cardinal directions).
const ANGLE_SNAP_STEP: f32 = std::f32::consts::FRAC_PI_2;

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

/// Width of the translucent curve guide lines (the bezier control polygon).
const GUIDE_LINE_WIDTH: f32 = ROAD_WIDTH * 0.2;

/// Height of the curve guide lines above the ground (avoids z-fighting roads).
const GUIDE_Y: f32 = 0.015;

/// Height of the curve control-point marker above the ground.
const CONTROL_Y: f32 = 0.02;

/// Radius of the curve control-point marker.
const CONTROL_POINT_RADIUS: f32 = 0.35;

/// Color of the curve guide lines and control point (light blue).
const GUIDE_RGB: (u8, u8, u8) = (130, 200, 255);

/// Shared meshes/materials for the road mechanic.
#[derive(Resource)]
pub(crate) struct RoadAssets {
    /// Single reused mesh handle for the preview ribbon (rebuilt each frame).
    preview_handle: Handle<Mesh>,
    /// Single reused mesh handle for the curve guide-line ribbon.
    guide_handle: Handle<Mesh>,
}

/// The committed road network: a set of polylines (`(x, z)` points).
#[derive(Resource, Default)]
pub(crate) struct RoadNetwork {
    chains: Vec<Vec<Vec2>>,
}

/// Which road tool is active.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub(crate) enum RoadMode {
    #[default]
    Straight,
    Curve,
}

/// Independent snapping toggles (like Cities: Skylines — non-exclusive).
#[derive(Resource)]
pub(crate) struct SnapSettings {
    /// Snap the cursor onto existing roads when close.
    snap_to_roads: bool,
    /// Snap a straight segment's direction to multiples of [`ANGLE_SNAP_STEP`].
    snap_to_angles: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            snap_to_roads: true,
            snap_to_angles: false,
        }
    }
}

/// Placement interaction state.
#[derive(Resource, Default)]
pub(crate) enum Placement {
    #[default]
    Neutral,
    /// Straight mode: `points` are the chain's committed vertices so far.
    Straight { points: Vec<Vec2> },
    /// Curve mode: start point set, waiting for the control point.
    CurveStart { p0: Vec2 },
    /// Curve mode: start + control set, waiting for the end point.
    Curve { p0: Vec2, p1: Vec2 },
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

/// On-screen HUD showing the road mode and snapping state.
#[derive(Component)]
pub(crate) struct HudMarker;

/// The translucent curve guide-line ribbon (control polygon).
#[derive(Component)]
pub(crate) struct GuideLines;

/// The marker at the curve's control point.
#[derive(Component)]
pub(crate) struct ControlPoint;

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
    let guide_material = solid_material(&mut materials, GUIDE_RGB, 0.75);

    let preview_handle = meshes.add(tessellate_chains(&[]));
    let guide_handle = meshes.add(tessellate_chains_at(&[], GUIDE_LINE_WIDTH, GUIDE_Y));

    commands.insert_resource(RoadAssets {
        preview_handle: preview_handle.clone(),
        guide_handle: guide_handle.clone(),
    });
    commands.insert_resource(RoadNetwork::default());
    commands.insert_resource(RoadMode::default());
    commands.insert_resource(SnapSettings::default());
    commands.insert_resource(Placement::default());

    // Ground plane (a large flat quad in the XZ plane at y = 0).
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::default()))),
        MeshMaterial3d(ground),
        Transform::from_scale(Vec3::new(GROUND_SIZE, 1.0, GROUND_SIZE)),
    ));

    // The merged road mesh (starts empty; rebuilt as roads are placed).
    commands.spawn((
        Mesh3d(meshes.add(tessellate_chains(&[]))),
        MeshMaterial3d(road_opaque),
        RoadMeshMarker,
        Transform::default(),
    ));

    // The preview ribbon (persistent; its mesh is rebuilt and it is shown/hidden
    // as the player places roads).
    commands.spawn((
        Mesh3d(preview_handle),
        MeshMaterial3d(road_preview),
        PreviewRoad,
        Transform::default(),
        Visibility::Hidden,
    ));

    // Curve guide lines + control point marker (hidden until placing a curve).
    commands.spawn((
        Mesh3d(guide_handle),
        MeshMaterial3d(guide_material.clone()),
        GuideLines,
        Transform::default(),
        Visibility::Hidden,
    ));
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Circle::new(CONTROL_POINT_RADIUS)))),
        MeshMaterial3d(guide_material),
        ControlPoint,
        Transform::from_xyz(0.0, CONTROL_Y, 0.0).with_rotation(flat),
        Visibility::Hidden,
    ));

    // Cursor: a flat circle (translucent fill) plus an opaque ring stroke,
    // parented so both move together. The marker holds the position.
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

/// Spawns the on-screen HUD (road mode + snapping state).
pub(crate) fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        HudMarker,
        Text::new("Mode: Straight\nSnap roads: On\nSnap angles: Off"),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.9, 0.9)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: px(8.0),
            right: px(8.0),
            padding: UiRect::all(px(6.0)),
            ..default()
        },
        ZIndex(1000),
    ));
}

/// Updates the HUD text from the current mode and snapping state.
pub(crate) fn update_hud(
    mode: Res<RoadMode>,
    snap: Res<SnapSettings>,
    mut hud: Single<&mut Text, With<HudMarker>>,
) {
    let mode_str = match *mode {
        RoadMode::Straight => "Straight",
        RoadMode::Curve => "Curve",
    };
    hud.0 = format!(
        "Mode: {mode_str}\nSnap roads: {}\nSnap angles: {}",
        if snap.snap_to_roads { "On" } else { "Off" },
        if snap.snap_to_angles { "On" } else { "Off" },
    );
}

/// Per-frame road placement: moves the cursor, handles clicks, and updates /
/// commits / finalizes roads.
///
/// Runs in `PostUpdate` after transform propagation so the camera's
/// `GlobalTransform` reflects this frame's pan/rotation.
pub(crate) fn road_placement_system(
    assets: Res<RoadAssets>,
    mut mode: ResMut<RoadMode>,
    mut snap: ResMut<SnapSettings>,
    mut placement: ResMut<Placement>,
    mut network: ResMut<RoadNetwork>,
    mut meshes: ResMut<Assets<Mesh>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    mut cursor_q: Query<&mut Transform, (With<CursorMarker>, Without<ControlPoint>)>,
    mut preview_vis_q: Query<
        &mut Visibility,
        (With<PreviewRoad>, Without<GuideLines>, Without<ControlPoint>),
    >,
    mut guide_vis_q: Query<
        &mut Visibility,
        (With<GuideLines>, Without<PreviewRoad>, Without<ControlPoint>),
    >,
    mut control_q: Query<
        (&mut Transform, &mut Visibility),
        (
            With<ControlPoint>,
            Without<PreviewRoad>,
            Without<GuideLines>,
            Without<CursorMarker>,
        ),
    >,
    mut road_mesh_q: Query<&mut Mesh3d, With<RoadMeshMarker>>,
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

    // Toggle snapping options and mode — always available, including during an
    // in-progress placement. Snapping toggles affect the current placement
    // immediately; toggling mode mid-placement cancels it (the state/mode
    // mismatch falls through the state machine below).
    if keys.just_pressed(KeyCode::KeyC) {
        *mode = match *mode {
            RoadMode::Straight => RoadMode::Curve,
            RoadMode::Curve => RoadMode::Straight,
        };
        info!("road mode: {:?}", *mode);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        snap.snap_to_roads = !snap.snap_to_roads;
        info!("snap to roads: {}", snap.snap_to_roads);
    }
    if keys.just_pressed(KeyCode::KeyG) {
        snap.snap_to_angles = !snap.snap_to_angles;
        info!("snap to angles: {}", snap.snap_to_angles);
    }

    // Snap the cursor: first to an existing road (if enabled), then constrain a
    // straight segment's direction to a snapped angle (if enabled).
    let road_snapped = if snap.snap_to_roads {
        snap_to_roads(ground, &network)
    } else {
        ground
    };
    let snapped = match &*placement {
        Placement::Straight { points } if snap.snap_to_angles => points
            .last()
            .map(|last| snap_angle(*last, road_snapped))
            .unwrap_or(road_snapped),
        _ => road_snapped,
    };
    if let Ok(mut tf) = cursor_q.single_mut() {
        tf.translation = Vec3::new(snapped.x, 0.0, snapped.y);
    }

    // Compute the preview polyline from the current state.
    let preview_polyline: Vec<Vec2> = match &*placement {
        Placement::Neutral => Vec::new(),
        Placement::Straight { points } => {
            let mut poly = points.clone();
            poly.push(snapped);
            poly
        }
        Placement::CurveStart { p0 } => vec![*p0, snapped],
        Placement::Curve { p0, p1 } => sample_quadratic_bezier(*p0, *p1, snapped),
    };
    update_preview(&assets, &mut meshes, &mut preview_vis_q, &preview_polyline);

    // Show the curve's control point and guide lines (the control polygon)
    // once the control point is placed; hide them otherwise.
    match &*placement {
        Placement::Curve { p0, p1 } => {
            if let Ok(mut vis) = guide_vis_q.single_mut() {
                *vis = Visibility::Visible;
            }
            if let Some(mut mesh) = meshes.get_mut(&assets.guide_handle) {
                let line0 = [*p0, *p1];
                let line1 = [*p1, snapped];
                *mesh = tessellate_chains_at(
                    &[line0.as_slice(), line1.as_slice()],
                    GUIDE_LINE_WIDTH,
                    GUIDE_Y,
                );
            }
            if let Ok((mut tf, mut vis)) = control_q.single_mut() {
                tf.translation = Vec3::new(p1.x, CONTROL_Y, p1.y);
                *vis = Visibility::Visible;
            }
        }
        _ => {
            if let Ok(mut vis) = guide_vis_q.single_mut() {
                *vis = Visibility::Hidden;
            }
            if let Ok((_, mut vis)) = control_q.single_mut() {
                *vis = Visibility::Hidden;
            }
        }
    }

    // Take ownership of the state, compute the next state, then put it back.
    let state = std::mem::replace(&mut *placement, Placement::Neutral);
    let new_state = match (state, *mode) {
        (Placement::Neutral, RoadMode::Straight) => {
            if buttons.just_pressed(MouseButton::Left) {
                split_chain_at(&mut network, snapped);
                Placement::Straight {
                    points: vec![snapped],
                }
            } else {
                Placement::Neutral
            }
        }
        (Placement::Straight { mut points }, RoadMode::Straight) => {
            if buttons.just_pressed(MouseButton::Left) {
                if points
                    .last()
                    .copied()
                    .is_none_or(|p| p.distance(snapped) > ROAD_WIDTH * 0.2)
                {
                    points.push(snapped);
                }
                rebuild_road_mesh(&network, &points, &mut meshes, &mut road_mesh_q);
                Placement::Straight { points }
            } else if buttons.just_pressed(MouseButton::Right) {
                if points.len() >= 2 {
                    network.chains.push(points);
                }
                rebuild_road_mesh(&network, &[], &mut meshes, &mut road_mesh_q);
                Placement::Neutral
            } else {
                Placement::Straight { points }
            }
        }
        (Placement::Neutral, RoadMode::Curve) => {
            if buttons.just_pressed(MouseButton::Left) {
                split_chain_at(&mut network, snapped);
                Placement::CurveStart { p0: snapped }
            } else {
                Placement::Neutral
            }
        }
        (Placement::CurveStart { p0 }, RoadMode::Curve) => {
            if buttons.just_pressed(MouseButton::Left) {
                Placement::Curve { p0, p1: snapped }
            } else if buttons.just_pressed(MouseButton::Right) {
                Placement::Neutral
            } else {
                Placement::CurveStart { p0 }
            }
        }
        (Placement::Curve { p0, p1 }, RoadMode::Curve) => {
            if buttons.just_pressed(MouseButton::Left) {
                network
                    .chains
                    .push(sample_quadratic_bezier(p0, p1, snapped));
                rebuild_road_mesh(&network, &[], &mut meshes, &mut road_mesh_q);
                // Chain the next curve from this one's end point (no re-start).
                Placement::CurveStart { p0: snapped }
            } else if buttons.just_pressed(MouseButton::Right) {
                Placement::Neutral
            } else {
                Placement::Curve { p0, p1 }
            }
        }
        // Mode switched mid-placement (C toggles at all times), so drop back to
        // neutral defensively.
        _ => Placement::Neutral,
    };

    *placement = new_state;
}

/// Rebuilds the preview mesh from `polyline` and shows/hides it.
fn update_preview(
    assets: &RoadAssets,
    meshes: &mut Assets<Mesh>,
    visibility_q: &mut Query<
        &mut Visibility,
        (With<PreviewRoad>, Without<GuideLines>, Without<ControlPoint>),
    >,
    polyline: &[Vec2],
) {
    let Ok(mut vis) = visibility_q.single_mut() else {
        return;
    };
    if polyline.len() >= 2 {
        *vis = Visibility::Visible;
        if let Some(mut mesh) = meshes.get_mut(&assets.preview_handle) {
            *mesh = tessellate_chains(&[polyline]);
        }
    } else {
        *vis = Visibility::Hidden;
    }
}

/// Rebuilds the merged road mesh from the finalized chains plus the active chain.
fn rebuild_road_mesh(
    network: &RoadNetwork,
    active: &[Vec2],
    meshes: &mut Assets<Mesh>,
    road_mesh_q: &mut Query<&mut Mesh3d, With<RoadMeshMarker>>,
) {
    let mut chains: Vec<&[Vec2]> = network.chains.iter().map(|c| c.as_slice()).collect();
    if active.len() >= 2 {
        chains.push(active);
    }
    let handle = meshes.add(tessellate_chains(&chains));
    if let Ok(mut mesh3d) = road_mesh_q.single_mut() {
        *mesh3d = Mesh3d(handle);
    }
}

/// Strokes every chain into one flat triangle mesh (round joins + caps) at
/// road width and the standard road height.
fn tessellate_chains(chains: &[&[Vec2]]) -> Mesh {
    tessellate_chains_at(chains, ROAD_WIDTH, ROAD_Y)
}

/// Strokes every chain into one flat triangle mesh (round joins + caps) with a
/// given width and height.
fn tessellate_chains_at(chains: &[&[Vec2]], width: f32, y: f32) -> Mesh {
    let mut geometry: VertexBuffers<Point, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    let options = StrokeOptions::tolerance(0.05)
        .with_line_width(width)
        .with_line_join(LineJoin::Round)
        .with_line_cap(LineCap::Round);

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
        .map(|p| [p.x, y, p.y])
        .collect();
    let normals: Vec<[f32; 3]> = positions.iter().map(|_| [0.0, 1.0, 0.0]).collect();
    let uvs: Vec<[f32; 2]> = geometry.vertices.iter().map(|p| [p.x, p.y]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(geometry.indices));
    mesh
}

/// Samples a quadratic Bezier `p0..p1..p2` into a polyline (adaptive density).
fn sample_quadratic_bezier(p0: Vec2, p1: Vec2, p2: Vec2) -> Vec<Vec2> {
    // Rough arc-length bound via the control polygon; ~4 samples per road width.
    let approx_len = p0.distance(p1) + p1.distance(p2);
    let segments = ((approx_len / (ROAD_WIDTH * 0.25)).ceil() as usize).clamp(4, 64);

    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let u = 1.0 - t;
        pts.push(u * u * p0 + 2.0 * u * t * p1 + t * t * p2);
    }
    pts
}

/// Snaps `end` so the segment `start..end` lies on a multiple of
/// [`ANGLE_SNAP_STEP`] (its direction is constrained, length tracks the cursor).
fn snap_angle(start: Vec2, end: Vec2) -> Vec2 {
    let dir = end - start;
    if dir.length_squared() < 1e-8 {
        return end;
    }
    let angle = dir.y.atan2(dir.x);
    let snapped_angle = (angle / ANGLE_SNAP_STEP).round() * ANGLE_SNAP_STEP;
    let snapped_dir = Vec2::new(snapped_angle.cos(), snapped_angle.sin());
    let length = dir.dot(snapped_dir);
    start + snapped_dir * length.max(0.0)
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
