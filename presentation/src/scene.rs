//! 3D scene: the board with round holes, discs that drop, the "thinking" disc,
//! win glow, lights, and an orbit camera that frames the board.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::game::{Game, GameFlow, Phase, Player, Settings};
use crate::sizes::BoardSize;

const HOLE_RADIUS: f32 = 0.41;
const DISC_RADIUS: f32 = 0.37;
const DISC_THICKNESS: f32 = 0.22;
const BOARD_THICKNESS: f32 = 0.44;
const MARGIN: f32 = 0.32;
/// Height of the board's bottom edge above the table (the feet fill the gap).
const LIFT: f32 = 0.45;
const HOLE_SEGMENTS: u32 = 32;
const GRAVITY: f32 = 28.0;
const BOUNCE: f32 = 0.28;
const SETTLE_SPEED: f32 = 1.2;
const POP_SECONDS: f32 = 0.45;
/// Pixels the UI covers on each side, used to frame the board in the free area.
const UI_SIDE_PX: f32 = 300.0;
const UI_TOP_PX: f32 = 115.0;
const UI_BOTTOM_PX: f32 = 175.0;

pub const RED: Color = Color::srgb(0.86, 0.13, 0.15);
pub const YELLOW: Color = Color::srgb(0.98, 0.76, 0.08);
const BOARD_BLUE: Color = Color::srgb(0.08, 0.27, 0.78);

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.035, 0.04, 0.07)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.75, 0.8, 1.0),
                brightness: 350.0,
                affects_lightmapped_meshes: true,
            })
            .insert_resource(BuiltBoard(None))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    rebuild_board,
                    pop_board,
                    fall,
                    hover_ghost,
                    glow,
                    orbit_camera,
                )
                    .after(GameFlow),
            );
    }
}

#[derive(Resource)]
pub struct SceneAssets {
    disc: Handle<Mesh>,
    solid: [Handle<StandardMaterial>; 2],
    glowing: [Handle<StandardMaterial>; 2],
    board: Handle<StandardMaterial>,
}

impl SceneAssets {
    pub fn glow_handles(&self) -> [Handle<StandardMaterial>; 2] {
        self.glowing.clone()
    }
}

#[derive(Resource)]
struct BuiltBoard(Option<BoardSize>);

#[derive(Component)]
struct BoardRoot;

#[derive(Component)]
struct Pop(f32);

#[derive(Component)]
pub struct Disc {
    pub col: u32,
    pub row: u32,
    pub player: Player,
}

#[derive(Component)]
pub struct Falling {
    delay: f32,
    /// Animation speed multiplier, so short move times still show the drop.
    speed: f32,
    velocity: f32,
    rest: f32,
}

#[derive(Component)]
struct Ghost(Player);

#[derive(Component)]
struct OrbitCamera {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    distance: f32,
    target: Vec3,
}

fn index(player: Player) -> usize {
    match player {
        Player::Red => 0,
        Player::Yellow => 1,
    }
}

fn to_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn cell_x(size: BoardSize, col: u32) -> f32 {
    to_f32(col) - (to_f32(size.columns) - 1.0) / 2.0
}

fn cell_y(row: u32) -> f32 {
    LIFT + MARGIN + 0.5 + to_f32(row)
}

fn board_top(size: BoardSize) -> f32 {
    LIFT + MARGIN * 2.0 + to_f32(size.rows)
}

fn board_centre(size: BoardSize) -> Vec3 {
    Vec3::new(0.0, board_top(size) / 2.0, 0.0)
}

fn disc_material(colour: Color, glow: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        perceptual_roughness: 0.35,
        metallic: 0.05,
        reflectance: 0.6,
        emissive: if glow { colour.to_linear() * 2.0 } else { LinearRgba::BLACK },
        ..default()
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let disc = meshes.add(Cylinder::new(DISC_RADIUS, DISC_THICKNESS));
    let ghost_material = |colour: Color| StandardMaterial {
        base_color: colour.with_alpha(0.55),
        alpha_mode: AlphaMode::Blend,
        emissive: colour.to_linear() * 0.6,
        ..default()
    };
    let assets = SceneAssets {
        disc: disc.clone(),
        solid: [
            materials.add(disc_material(RED, false)),
            materials.add(disc_material(YELLOW, false)),
        ],
        glowing: [
            materials.add(disc_material(RED, true)),
            materials.add(disc_material(YELLOW, true)),
        ],
        board: materials.add(StandardMaterial {
            base_color: BOARD_BLUE,
            perceptual_roughness: 0.3,
            metallic: 0.1,
            reflectance: 0.5,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    };
    for player in [Player::Red, Player::Yellow] {
        let colour = match player {
            Player::Red => RED,
            Player::Yellow => YELLOW,
        };
        commands.spawn((
            Ghost(player),
            Mesh3d(disc.clone()),
            MeshMaterial3d(materials.add(ghost_material(colour))),
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(assets);

    // Table.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(40.0, 0.2, 18.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.13, 0.1, 0.09),
            perceptual_roughness: 0.8,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.1, 0.0),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-4.0, 9.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PointLight {
            intensity: 400_000.0,
            range: 40.0,
            color: Color::srgb(0.6, 0.7, 1.0),
            ..default()
        },
        Transform::from_xyz(5.0, 6.0, -6.0),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 5.0, 16.0).looking_at(Vec3::new(0.0, 4.0, 0.0), Vec3::Y),
        OrbitCamera {
            yaw: 0.0,
            pitch: 0.12,
            zoom: 1.0,
            distance: 18.0,
            target: Vec3::new(0.0, 4.0, 0.0),
        },
    ));
}

/// One cell: a square plate with a round hole, front face, back face and hole wall.
fn push_cell(centre: Vec2, positions: &mut Vec<[f32; 3]>, normals: &mut Vec<[f32; 3]>, indices: &mut Vec<u32>) {
    let half = BOARD_THICKNESS / 2.0;
    let ring = |step: u32| {
        let angle = TAU * to_f32(step) / to_f32(HOLE_SEGMENTS);
        let dir = Vec2::new(angle.cos(), angle.sin());
        let square = dir * (0.5 / dir.x.abs().max(dir.y.abs()));
        (centre + square, centre + dir * HOLE_RADIUS, dir)
    };
    let mut quad = |corners: [[f32; 3]; 4], normal: [f32; 3]| {
        let base = u32::try_from(positions.len()).unwrap_or(0);
        positions.extend(corners);
        normals.extend([normal; 4]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    for step in 0..HOLE_SEGMENTS {
        let (outer_a, inner_a, dir_a) = ring(step);
        let (outer_b, inner_b, dir_b) = ring(step + 1);
        quad(
            [
                [outer_a.x, outer_a.y, half],
                [outer_b.x, outer_b.y, half],
                [inner_b.x, inner_b.y, half],
                [inner_a.x, inner_a.y, half],
            ],
            [0.0, 0.0, 1.0],
        );
        quad(
            [
                [inner_a.x, inner_a.y, -half],
                [inner_b.x, inner_b.y, -half],
                [outer_b.x, outer_b.y, -half],
                [outer_a.x, outer_a.y, -half],
            ],
            [0.0, 0.0, -1.0],
        );
        let inward = -(dir_a + dir_b).normalize();
        quad(
            [
                [inner_a.x, inner_a.y, half],
                [inner_b.x, inner_b.y, half],
                [inner_b.x, inner_b.y, -half],
                [inner_a.x, inner_a.y, -half],
            ],
            [inward.x, inward.y, 0.0],
        );
    }
}

fn plate_mesh(size: BoardSize) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for col in 0..size.columns {
        for row in 0..size.rows {
            push_cell(Vec2::new(cell_x(size, col), cell_y(row)), &mut positions, &mut normals, &mut indices);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

fn rebuild_board(
    mut commands: Commands,
    game: Res<Game>,
    mut built: ResMut<BuiltBoard>,
    assets: Res<SceneAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<Entity, With<BoardRoot>>,
) {
    if built.0 == Some(game.size) {
        return;
    }
    let grows = built.0.is_none_or(|before| game.size.columns * game.size.rows >= before.columns * before.rows);
    built.0 = Some(game.size);
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let size = game.size;
    let width = to_f32(size.columns);
    let height = to_f32(size.rows);
    let outer_width = width + MARGIN * 2.0;
    let mid_y = LIFT + MARGIN + height / 2.0;
    let mut bar = |w: f32, h: f32, x: f32, y: f32| {
        (
            Mesh3d(meshes.add(Cuboid::new(w, h, BOARD_THICKNESS))),
            MeshMaterial3d(assets.board.clone()),
            Transform::from_xyz(x, y, 0.0),
        )
    };
    let side_x = width / 2.0 + MARGIN / 2.0;
    let parts = [
        bar(MARGIN, height + MARGIN * 2.0, -side_x, mid_y),
        bar(MARGIN, height + MARGIN * 2.0, side_x, mid_y),
        bar(outer_width, MARGIN, 0.0, LIFT + MARGIN / 2.0),
        bar(outer_width, MARGIN * 0.6, 0.0, board_top(size) - MARGIN * 0.3),
    ];
    let feet = [-1.0_f32, 1.0].map(|sign| {
        (
            Mesh3d(meshes.add(Cuboid::new(0.35, LIFT + 1.2, 1.6))),
            MeshMaterial3d(assets.board.clone()),
            Transform::from_xyz(sign * (side_x + 0.05), (LIFT + 1.2) / 2.0, 0.0),
        )
    });
    let plate = meshes.add(plate_mesh(size));
    let start = if grows { 0.0 } else { 0.5 };
    commands
        .spawn((
            BoardRoot,
            Pop(start),
            Transform::from_scale(Vec3::splat(0.6)),
            Visibility::default(),
        ))
        .with_children(|root| {
            root.spawn((Mesh3d(plate), MeshMaterial3d(assets.board.clone()), Transform::default()));
            for part in parts {
                root.spawn(part);
            }
            for foot in feet {
                root.spawn(foot);
            }
        });
}

/// easeOutBack: overshoots a little, then settles at 1.
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u.powi(3) + c1 * u.powi(2)
}

fn pop_board(mut commands: Commands, time: Res<Time>, mut boards: Query<(Entity, &mut Pop, &mut Transform)>) {
    for (entity, mut pop, mut transform) in &mut boards {
        pop.0 = (pop.0 + time.delta_secs() / POP_SECONDS).min(1.0);
        let scale = 0.6 + 0.4 * ease_out_back(pop.0);
        // Scale about the base so the board grows up from the table.
        transform.scale = Vec3::splat(scale);
        if pop.0 >= 1.0 {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<Pop>();
        }
    }
}

/// Spawns a disc. With `speed` it drops in (after `delay`) at that animation speed;
/// with `None` it appears in its cell straight away (used for very fast games).
pub fn spawn_disc(commands: &mut Commands, assets: &SceneAssets, size: BoardSize, player: Player, cell: (u32, u32), delay: f32, speed: Option<f32>) {
    let (col, row) = cell;
    let rotation = Quat::from_rotation_x(FRAC_PI_2);
    let mut disc = commands.spawn((
        Disc { col, row, player },
        Mesh3d(assets.disc.clone()),
        MeshMaterial3d(assets.solid[index(player)].clone()),
    ));
    match speed {
        Some(speed) => {
            disc.insert((
                Falling {
                    delay,
                    speed,
                    velocity: 0.0,
                    rest: cell_y(row),
                },
                Transform::from_xyz(cell_x(size, col), board_top(size) + 1.0, 0.0).with_rotation(rotation),
                if delay > 0.0 { Visibility::Hidden } else { Visibility::Inherited },
            ));
        }
        None => {
            disc.insert((Transform::from_xyz(cell_x(size, col), cell_y(row), 0.0).with_rotation(rotation), Visibility::Inherited));
        }
    }
}

pub fn light_up(assets: &SceneAssets, disc: &Disc, material: &mut MeshMaterial3d<StandardMaterial>) {
    material.0 = assets.glowing[index(disc.player)].clone();
}

fn fall(mut commands: Commands, time: Res<Time>, mut discs: Query<(Entity, &mut Falling, &mut Transform, &mut Visibility)>) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (entity, mut falling, mut transform, mut visibility) in &mut discs {
        if falling.delay > 0.0 {
            falling.delay -= dt;
            continue;
        }
        let dt = dt * falling.speed;
        *visibility = Visibility::Inherited;
        falling.velocity -= GRAVITY * dt;
        transform.translation.y += falling.velocity * dt;
        if transform.translation.y <= falling.rest {
            transform.translation.y = falling.rest;
            if falling.velocity.abs() * BOUNCE > SETTLE_SPEED {
                falling.velocity = -falling.velocity * BOUNCE;
            } else {
                commands.entity(entity).remove::<Falling>();
            }
        }
    }
}

fn hover_ghost(game: Res<Game>, settings: Res<Settings>, time: Res<Time>, mut ghosts: Query<(&Ghost, &mut Transform, &mut Visibility)>) {
    let t = time.elapsed_secs();
    let thinking = game.waiting && settings.animated() && matches!(game.phase, Phase::Running | Phase::Paused);
    let span = (to_f32(game.size.columns) - 1.0) / 2.0;
    for (ghost, mut transform, mut visibility) in &mut ghosts {
        let shown = thinking && ghost.0 == game.to_move;
        *visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
        transform.translation = Vec3::new(
            (t * 1.6).sin() * span * 0.85,
            board_top(game.size) + 0.9 + (t * 4.0).sin() * 0.12,
            0.0,
        );
    }
}

fn glow(time: Res<Time>, assets: Res<SceneAssets>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let pulse = 1.2 + (time.elapsed_secs() * 5.0).sin() * 1.0;
    for (handle, colour) in assets.glowing.iter().zip([RED, YELLOW]) {
        if let Some(mut material) = materials.get_mut(handle) {
            material.emissive = colour.to_linear() * pulse;
        }
    }
}

fn orbit_camera(
    time: Res<Time>,
    game: Res<Game>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window>,
    mut cameras: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((mut orbit, mut transform)) = cameras.single_mut() else {
        return;
    };
    if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.004).clamp(-0.1, 1.2);
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 60.0,
    };
    orbit.zoom = (orbit.zoom * (1.0 - lines * 0.08)).clamp(0.45, 2.5);

    // Fit the board into the part of the window the UI does not cover.
    let size = game.size;
    let width_px = window.width().max(1.0);
    let height_px = window.height().max(1.0);
    let free_w = (width_px - UI_SIDE_PX * 2.0).max(width_px * 0.35);
    let free_h = (height_px - UI_TOP_PX - UI_BOTTOM_PX).max(height_px * 0.35);
    let half_fov = (FRAC_PI_4 / 2.0).tan();
    let board_w = to_f32(size.columns) + MARGIN * 2.0 + 0.6;
    let board_h = board_top(size) + 1.6;
    let fit = (board_w * height_px / (2.0 * half_fov * free_w)).max(board_h * height_px / (2.0 * half_fov * free_h)) * 1.08;
    let wanted = fit * orbit.zoom;
    let blend = 1.0 - (-time.delta_secs() * 4.0).exp();
    orbit.distance += (wanted - orbit.distance) * blend;
    let world_per_px = 2.0 * orbit.distance * half_fov / height_px;
    let shift = (UI_BOTTOM_PX - UI_TOP_PX) / 2.0 * world_per_px;
    let target = board_centre(size) + Vec3::new(0.0, 0.5 - shift, 0.0);
    orbit.target = orbit.target.lerp(target, blend);

    let sway = (time.elapsed_secs() * 0.25).sin() * 0.12;
    let yaw = orbit.yaw + sway;
    let offset = Vec3::new(
        orbit.distance * orbit.pitch.cos() * yaw.sin(),
        orbit.distance * orbit.pitch.sin(),
        orbit.distance * orbit.pitch.cos() * yaw.cos(),
    );
    *transform = Transform::from_translation(orbit.target + offset).looking_at(orbit.target, Vec3::Y);
}
