//! 3D scene: the board with round holes, discs that drop, the "thinking" disc,
//! win glow, lights, and an orbit camera that frames the board.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::mesh::Indices;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::game::{Game, GameFlow, Phase, Player, Settings};
use crate::present::Mode;
use crate::sizes::BoardSize;
use crate::theme::{BG, BOARD, RED, TABLE, YELLOW};

const HOLE_RADIUS: f32 = 0.43;
/// The chamfer from each face into a hole.
const BEVEL: f32 = 0.05;
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
/// Logical pixels the arena UI covers on each edge of the window. `ui.rs` measures
/// its panels every frame; the camera frames the boards in what is left, so no board
/// ever sits behind a panel.
#[derive(Resource, Clone, Copy)]
pub struct UiMargins {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for UiMargins {
    fn default() -> Self {
        Self {
            left: 330.0,
            right: 330.0,
            top: 110.0,
            bottom: 200.0,
        }
    }
}


pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(BG))
            .init_resource::<UiMargins>()
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(1.0, 0.9, 0.78),
                brightness: 350.0,
                affects_lightmapped_meshes: true,
            })
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (rebuild_board, morph_boards, shape_boards).chain(),
                    pop_board,
                    glide_tables,
                    fall,
                    thud,
                    hover_ghost,
                    glow,
                    orbit_camera.run_if(in_state(Mode::Arena)),
                )
                    .after(GameFlow),
            );
    }
}

#[derive(Resource)]
pub struct SceneAssets {
    /// The ridged piece, and a plain low-poly one for many boards or very fast games
    /// (hundreds of discs at full detail are too much for an integrated GPU).
    pub disc: Handle<Mesh>,
    pub disc_simple: Handle<Mesh>,
    /// One cell tile, and a unit cube for frame pieces.
    cell: Handle<Mesh>,
    cube: Handle<Mesh>,
    pub solid: [Handle<StandardMaterial>; 2],
    glowing: [Handle<StandardMaterial>; 2],
    pub board: Handle<StandardMaterial>,
}

impl SceneAssets {
}

/// On a table: the size its board is (or is morphing to), its root entity, and the
/// cells that exist (the larger of old and new while a resize is animating).
#[derive(Component)]
struct Built {
    size: BoardSize,
    root: Entity,
    cells: BoardSize,
}

/// Where a table should stand; the scene eases it there when the layout changes.
#[derive(Component)]
pub struct Spot(pub Vec3);

/// On a table while its board changes size; the game waits for it before moving.
#[derive(Component)]
pub struct Morphing;

/// A board's current shape in cells; fractional while it morphs.
#[derive(Component)]
pub struct Shape {
    pub w: f32,
    pub h: f32,
}

/// A resize in progress, from the shape it started at.
#[derive(Component)]
struct Morph {
    from: (f32, f32),
    to: BoardSize,
    t: f32,
}

/// One cell tile of an arena board (a square with a round hole).
#[derive(Component)]
struct Cell {
    col: u32,
    row: u32,
}

/// The frame pieces of an arena board; unit cubes stretched by `shape_boards`.
#[derive(Component, Clone, Copy)]
enum Frame {
    Left,
    Right,
    Bottom,
    Top,
    FootLeft,
    FootRight,
}

#[derive(Component)]
struct BoardRoot;

/// A board's little rock back when a disc lands in it.
#[derive(Component)]
struct Thud {
    age: f32,
    strength: f32,
}

#[derive(Component)]
struct Pop(f32);

#[derive(Component)]
pub struct Disc {
    /// The table (board) this disc belongs to.
    pub table: usize,
    pub col: u32,
    pub row: u32,
}

impl Disc {
    /// A pooled disc not in play: it matches no cell.
    pub const fn spare(table: usize) -> Self {
        Self {
            table,
            col: u32::MAX,
            row: u32::MAX,
        }
    }
}

#[derive(Component)]
pub struct Falling {
    delay: f32,
    /// Where the drop starts once the delay is over (it waits out of sight until then).
    start: f32,
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
    /// Extra pitch for several rows of boards, eased in.
    lift: f32,
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

pub fn cell_x(size: BoardSize, col: u32) -> f32 {
    to_f32(col) - (to_f32(size.columns) - 1.0) / 2.0
}

pub fn cell_y(row: u32) -> f32 {
    LIFT + MARGIN + 0.5 + to_f32(row)
}

pub fn board_top(size: BoardSize) -> f32 {
    LIFT + MARGIN * 2.0 + to_f32(size.rows)
}

fn board_centre(size: BoardSize) -> Vec3 {
    Vec3::new(0.0, board_top(size) / 2.0, 0.0)
}

/// Glossy plastic: a smooth clear coat over a slightly rougher colour layer.
fn disc_material(colour: Color, glow: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        perceptual_roughness: 0.28,
        metallic: 0.0,
        reflectance: 0.6,
        clearcoat: 1.0,
        clearcoat_perceptual_roughness: 0.08,
        emissive: if glow { colour.to_linear() * 2.0 } else { LinearRgba::BLACK },
        // The lathe's normals are exact; draw both windings but never flip them.
        double_sided: false,
        cull_mode: None,
        ..default()
    }
}

/// A Connect Four piece, turned on a lathe about y: a raised outer rim, a recessed
/// face with two concentric ridges, and rounded edges. Flat-shaded per band so the
/// ridges catch the light.
fn disc_mesh() -> Mesh {
    let half = DISC_THICKNESS / 2.0;
    let face = half * 0.62;
    let ridge = face + 0.012;
    let r = DISC_RADIUS;
    // (radius, height) along the top, from the centre to the outer edge.
    let top: [(f32, f32); 17] = [
        (0.0, face),
        (0.095, face),
        (0.102, ridge),
        (0.118, ridge),
        (0.125, face),
        (0.175, face),
        (0.182, ridge),
        (0.198, ridge),
        (0.205, face),
        (r - 0.11, face),
        (r - 0.085, half),
        (r - 0.035, half),
        (r - 0.012, half - 0.01),
        (r - 0.002, half - 0.03),
        (r, half - 0.05),
        (r, 0.0),
        (r, 0.0),
    ];
    let mut profile: Vec<(f32, f32)> = top[..15].to_vec();
    profile.extend(top[..15].iter().rev().map(|&(radius, y)| (radius, -y)));
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let segments = 28_u32;
    for pair in profile.windows(2) {
        let ((r0, y0), (r1, y1)) = (pair[0], pair[1]);
        let (dr, dy) = (r1 - r0, y1 - y0);
        let length = dr.hypot(dy);
        if length < 1e-5 {
            continue;
        }
        let (nr, ny) = (-dy / length, dr / length);
        for step in 0..segments {
            let a0 = TAU * to_f32(step) / to_f32(segments);
            let a1 = TAU * to_f32(step + 1) / to_f32(segments);
            let base = u32::try_from(positions.len()).unwrap_or(0);
            for (radius, y, angle) in [(r0, y0, a0), (r1, y1, a0), (r1, y1, a1), (r0, y0, a1)] {
                positions.push([radius * angle.cos(), y, radius * angle.sin()]);
                normals.push([nr * angle.cos(), ny, nr * angle.sin()]);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let disc = meshes.add(disc_mesh());
    let ghost_material = |colour: Color| StandardMaterial {
        base_color: colour.with_alpha(0.55),
        alpha_mode: AlphaMode::Blend,
        emissive: colour.to_linear() * 0.6,
        ..default()
    };
    let assets = SceneAssets {
        disc: disc.clone(),
        disc_simple: meshes.add(Cylinder::new(DISC_RADIUS, DISC_THICKNESS).mesh().resolution(20)),
        cell: meshes.add(cell_mesh()),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        solid: [
            materials.add(disc_material(RED, false)),
            materials.add(disc_material(YELLOW, false)),
        ],
        glowing: [
            materials.add(disc_material(RED, true)),
            materials.add(disc_material(YELLOW, true)),
        ],
        board: materials.add(StandardMaterial {
            base_color: BOARD,
            perceptual_roughness: 0.32,
            metallic: 0.0,
            reflectance: 0.5,
            clearcoat: 0.6,
            clearcoat_perceptual_roughness: 0.18,
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
        Mesh3d(meshes.add(Cuboid::new(90.0, 0.2, 50.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: TABLE,
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
        // Two cascades over a short range: every shadow caster is drawn once per cascade.
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            maximum_distance: 70.0,
            first_cascade_far_bound: 20.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(-4.0, 9.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PointLight {
            intensity: 400_000.0,
            range: 40.0,
            color: Color::srgb(1.0, 0.82, 0.62),
            ..default()
        },
        Transform::from_xyz(5.0, 6.0, -6.0),
    ));

    commands.spawn((
        Camera3d::default(),
        // The flat UI draws on this camera, not the 3D HUD's (which is off in backup mode).
        IsDefaultUiCamera,
        Msaa::Off,
        Fxaa::default(),
        // Soft bloom so glows, flashes and the win beam really shine.
        Bloom {
            intensity: 0.12,
            ..Bloom::NATURAL
        },
        Transform::from_xyz(0.0, 5.0, 16.0).looking_at(Vec3::new(0.0, 4.0, 0.0), Vec3::Y),
        OrbitCamera {
            yaw: 0.0,
            pitch: 0.12,
            zoom: 1.0,
            distance: 18.0,
            target: Vec3::new(0.0, 4.0, 0.0),
            lift: 0.0,
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
        // The hole: a bevel into it from each face, and the wall between.
        let inward = -(dir_a + dir_b).normalize();
        let deep = half - BEVEL;
        let (wall_a, wall_b) = (centre + dir_a * (HOLE_RADIUS - BEVEL), centre + dir_b * (HOLE_RADIUS - BEVEL));
        let slope = (inward * 0.7).extend(0.7);
        quad(
            [
                [inner_a.x, inner_a.y, half],
                [inner_b.x, inner_b.y, half],
                [wall_b.x, wall_b.y, deep],
                [wall_a.x, wall_a.y, deep],
            ],
            slope.to_array(),
        );
        quad(
            [
                [wall_a.x, wall_a.y, deep],
                [wall_b.x, wall_b.y, deep],
                [wall_b.x, wall_b.y, -deep],
                [wall_a.x, wall_a.y, -deep],
            ],
            [inward.x, inward.y, 0.0],
        );
        quad(
            [
                [wall_a.x, wall_a.y, -deep],
                [wall_b.x, wall_b.y, -deep],
                [inner_b.x, inner_b.y, -half],
                [inner_a.x, inner_a.y, -half],
            ],
            [slope.x, slope.y, -slope.z],
        );
    }
}

/// A single 1 x 1 cell tile centred on the origin.
fn cell_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    push_cell(Vec2::ZERO, &mut positions, &mut normals, &mut indices);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
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

/// Builds each table's board the first time, and starts a morph when its size changes.
///
/// Arena boards are made of one tile per cell plus a frame of unit cubes, so a resize
/// can animate: the frame slides outward while new columns unfold, then the top rises
/// while new rows grow in (shrinking runs the other way round).
fn rebuild_board(
    mut commands: Commands,
    mut tables: Query<(Entity, &Game, Option<&mut Built>)>,
    mut roots: Query<(&mut Shape, Option<&mut Morph>)>,
    assets: Res<SceneAssets>,
) {
    for (table, game, built) in &mut tables {
        let size = game.size;
        let Some(mut built) = built else {
            let root = commands
                .spawn((
                    BoardRoot,
                    Pop(0.0),
                    Shape { w: to_f32(size.columns), h: to_f32(size.rows) },
                    Transform::from_scale(Vec3::splat(0.6)),
                    Visibility::default(),
                    ChildOf(table),
                ))
                .id();
            spawn_board_pieces(&mut commands, &assets, root, size, None);
            commands.entity(table).insert(Built { size, root, cells: size });
            continue;
        };
        if built.size == size {
            continue;
        }
        let Ok((shape, morph)) = roots.get_mut(built.root) else {
            continue;
        };
        // Add the cells the new size needs that the board does not have yet.
        let have = built.cells;
        let grown = BoardSize {
            columns: have.columns.max(size.columns),
            rows: have.rows.max(size.rows),
            label: size.label,
        };
        spawn_board_pieces(&mut commands, &assets, built.root, grown, Some(have));
        built.cells = grown;
        built.size = size;
        let fresh = Morph {
            from: (shape.w, shape.h),
            to: size,
            t: 0.0,
        };
        match morph {
            Some(mut morph) => *morph = fresh,
            None => {
                commands.entity(built.root).insert(fresh);
            }
        }
        commands.entity(table).insert(Morphing);
    }
}

/// Spawns the cell tiles of `size` that are not in `skip` (and the frame when `skip` is
/// `None`, i.e. a new board). Positions are set by `shape_boards`.
fn spawn_board_pieces(commands: &mut Commands, assets: &SceneAssets, root: Entity, size: BoardSize, skip: Option<BoardSize>) {
    for col in 0..size.columns {
        for row in 0..size.rows {
            if skip.is_some_and(|old| col < old.columns && row < old.rows) {
                continue;
            }
            commands.spawn((
                Cell { col, row },
                Mesh3d(assets.cell.clone()),
                MeshMaterial3d(assets.board.clone()),
                Transform::from_scale(Vec3::ZERO),
                ChildOf(root),
            ));
        }
    }
    if skip.is_none() {
        for piece in [Frame::Left, Frame::Right, Frame::Bottom, Frame::Top, Frame::FootLeft, Frame::FootRight] {
            commands.spawn((
                piece,
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.board.clone()),
                Transform::from_scale(Vec3::ZERO),
                ChildOf(root),
            ));
        }
    }
}

/// Builds a morphable board (cell tiles plus a frame) under `root`, for the slides.
pub fn spawn_shaped_board(commands: &mut Commands, assets: &SceneAssets, root: Entity, size: BoardSize) {
    spawn_board_pieces(commands, assets, root, size, None);
    commands.entity(root).insert(Shape {
        w: to_f32(size.columns),
        h: to_f32(size.rows),
    });
}

/// Starts a slide board morphing from its current shape to `to` (it has cells for `have`).
pub fn morph_shaped_board(commands: &mut Commands, assets: &SceneAssets, root: Entity, from: (f32, f32), have: BoardSize, to: BoardSize) {
    let grown = BoardSize {
        columns: have.columns.max(to.columns),
        rows: have.rows.max(to.rows),
        label: to.label,
    };
    spawn_board_pieces(commands, assets, root, grown, Some(have));
    commands.entity(root).insert(Morph { from, to, t: 0.0 });
}

const MORPH_SECONDS: f32 = 1.3;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Advances resize morphs: width first then height when growing, the reverse when shrinking.
fn morph_boards(
    mut commands: Commands,
    time: Res<Time>,
    mut roots: Query<(Entity, &mut Morph, &mut Shape)>,
    mut tables: Query<(Entity, &mut Built)>,
    cells: Query<(Entity, &Cell, &ChildOf)>,
) {
    for (root, mut morph, mut shape) in &mut roots {
        morph.t = (morph.t + time.delta_secs() / MORPH_SECONDS).min(1.0);
        let (from_w, from_h) = morph.from;
        let to_w = to_f32(morph.to.columns);
        let to_h = to_f32(morph.to.rows);
        let growing = to_w * to_h >= from_w * from_h;
        let first = smooth(morph.t * 2.0);
        let second = smooth(morph.t * 2.0 - 1.0);
        let (pw, ph) = if growing { (first, second) } else { (second, first) };
        shape.w = from_w + (to_w - from_w) * pw;
        shape.h = from_h + (to_h - from_h) * ph;
        if morph.t >= 1.0 {
            // Drop the cells the board no longer needs.
            for (entity, cell, parent) in &cells {
                if parent.parent() == root && (cell.col >= morph.to.columns || cell.row >= morph.to.rows) {
                    commands.entity(entity).despawn();
                }
            }
            for (table, mut built) in &mut tables {
                if built.root == root {
                    built.cells = morph.to;
                    commands.entity(table).remove::<Morphing>();
                }
            }
            commands.entity(root).remove::<Morph>();
        }
    }
}

/// Lays out every board's cells and frame for its current (possibly fractional) shape.
fn shape_boards(
    roots: Query<(Entity, &Shape), Changed<Shape>>,
    mut cells: Query<(&Cell, &ChildOf, &mut Transform), Without<Frame>>,
    mut frames: Query<(&Frame, &ChildOf, &mut Transform), Without<Cell>>,
) {
    for (root, shape) in &roots {
        let (w, h) = (shape.w, shape.h);
        for (cell, parent, mut transform) in &mut cells {
            if parent.parent() != root {
                continue;
            }
            // A cell past the edge is partly unfolded: as wide/tall as the space it has.
            let cw = (w - to_f32(cell.col)).clamp(0.0, 1.0);
            let ch = (h - to_f32(cell.row)).clamp(0.0, 1.0);
            // Folded cells sit at the edge they fold into, so nothing floats outside the frame.
            let x = to_f32(cell.col).min(w) - w / 2.0 + cw / 2.0;
            let y = LIFT + MARGIN + to_f32(cell.row).min(h) + ch / 2.0;
            transform.translation = Vec3::new(x, y, 0.0);
            transform.scale = if cw < 0.001 || ch < 0.001 { Vec3::ZERO } else { Vec3::new(cw, ch, 1.0) };
        }
        let side_x = w / 2.0 + MARGIN / 2.0;
        let outer_w = w + MARGIN * 2.0;
        let foot_h = LIFT + 1.2;
        for (piece, parent, mut transform) in &mut frames {
            if parent.parent() != root {
                continue;
            }
            let (at, scale) = match piece {
                Frame::Left => (Vec3::new(-side_x, LIFT + MARGIN + h / 2.0, 0.0), Vec3::new(MARGIN, h + MARGIN * 2.0, BOARD_THICKNESS)),
                Frame::Right => (Vec3::new(side_x, LIFT + MARGIN + h / 2.0, 0.0), Vec3::new(MARGIN, h + MARGIN * 2.0, BOARD_THICKNESS)),
                Frame::Bottom => (Vec3::new(0.0, LIFT + MARGIN / 2.0, 0.0), Vec3::new(outer_w, MARGIN, BOARD_THICKNESS)),
                Frame::Top => (
                    Vec3::new(0.0, LIFT + MARGIN * 2.0 + h - MARGIN * 0.3, 0.0),
                    Vec3::new(outer_w, MARGIN * 0.6, BOARD_THICKNESS),
                ),
                Frame::FootLeft => (Vec3::new(-(side_x + 0.05), foot_h / 2.0, 0.0), Vec3::new(0.35, foot_h, 1.6)),
                Frame::FootRight => (Vec3::new(side_x + 0.05, foot_h / 2.0, 0.0), Vec3::new(0.35, foot_h, 1.6)),
            };
            transform.translation = at;
            transform.scale = scale;
        }
    }
}

/// Columns and rows of the table grid for `count` boards.
pub fn grid(count: usize) -> (usize, usize) {
    match count {
        0 | 1 => (1, 1),
        2 => (2, 1),
        3 => (3, 1),
        4 => (2, 2),
        5 | 6 => (3, 2),
        7..=9 => (3, 3),
        _ => {
            // A wall a little wider than tall, like the free part of the window.
            let columns = (usize_f32(count) * 1.6).sqrt().ceil().max(1.0);
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "small positive count")]
            let columns = columns as usize;
            (columns, count.div_ceil(columns))
        }
    }
}

/// Up to nine boards stand in rows going back; more than that stand in a flat wall,
/// rows stacked upward, so every board faces the camera.
pub const fn stacked(count: usize) -> bool {
    count > 9
}

/// Height of one row in a wall of boards.
fn wall_step(size: BoardSize) -> f32 {
    board_top(size) + TABLE_GAP
}

/// Gap between boards side by side, and the distance between rows (front to back).
const TABLE_GAP: f32 = 1.6;
const ROW_DEPTH: f32 = 5.5;

/// Where table `index` of `count` stands: rows go back from the front, centred on x and z.
pub fn table_offset(index: usize, count: usize, size: BoardSize) -> Vec3 {
    let (columns, rows) = grid(count);
    let column = index % columns;
    let row = index / columns;
    let step_x = to_f32(size.columns) + MARGIN * 2.0 + TABLE_GAP;
    let x = (usize_f32(column) - (usize_f32(columns) - 1.0) / 2.0) * step_x;
    if stacked(count) {
        // Top row first, the bottom row stands on the table.
        let y = (usize_f32(rows) - 1.0 - usize_f32(row)) * wall_step(size);
        return Vec3::new(x, y, 0.0);
    }
    let z = ((usize_f32(rows) - 1.0) / 2.0 - usize_f32(row)) * ROW_DEPTH;
    Vec3::new(x, 0.0, z)
}

fn usize_f32(value: usize) -> f32 {
    to_f32(u32::try_from(value).unwrap_or(u32::MAX))
}

/// Everything a board of this size is made of (plate with holes, frame, feet), in
/// board space: centred on x, standing on y = 0, holes at `cell_x`/`cell_y`.
pub fn board_parts(
    size: BoardSize,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
) -> Vec<(Mesh3d, MeshMaterial3d<StandardMaterial>, Transform)> {
    let width = to_f32(size.columns);
    let height = to_f32(size.rows);
    let outer_width = width + MARGIN * 2.0;
    let mid_y = LIFT + MARGIN + height / 2.0;
    let mut part = |mesh: Mesh, at: Vec3| (Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_translation(at));
    let side_x = width / 2.0 + MARGIN / 2.0;
    let mut parts = vec![
        part(plate_mesh(size), Vec3::ZERO),
        part(Cuboid::new(MARGIN, height + MARGIN * 2.0, BOARD_THICKNESS).into(), Vec3::new(-side_x, mid_y, 0.0)),
        part(Cuboid::new(MARGIN, height + MARGIN * 2.0, BOARD_THICKNESS).into(), Vec3::new(side_x, mid_y, 0.0)),
        part(Cuboid::new(outer_width, MARGIN, BOARD_THICKNESS).into(), Vec3::new(0.0, LIFT + MARGIN / 2.0, 0.0)),
        part(
            Cuboid::new(outer_width, MARGIN * 0.6, BOARD_THICKNESS).into(),
            Vec3::new(0.0, board_top(size) - MARGIN * 0.3, 0.0),
        ),
    ];
    for sign in [-1.0_f32, 1.0] {
        parts.push(part(
            Cuboid::new(0.35, LIFT + 1.2, 1.6).into(),
            Vec3::new(sign * (side_x + 0.05), (LIFT + 1.2) / 2.0, 0.0),
        ));
    }
    parts
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

/// Puts a pooled disc entity into play on its table. With `speed` it drops in (after
/// `delay`) at that animation speed; with `None` it appears in its cell straight away.
#[allow(clippy::too_many_arguments)]
pub fn place_disc(
    commands: &mut Commands,
    disc: Entity,
    table: usize,
    size: BoardSize,
    cell: (u32, u32),
    delay: f32,
    speed: Option<f32>,
) {
    let (col, row) = cell;
    let rotation = Quat::from_rotation_x(FRAC_PI_2);
    let mut disc = commands.entity(disc);
    // Mesh and material were set when the disc was spawned or by `dress_disc`.
    disc.insert(Disc { table, col, row });
    match speed {
        Some(speed) => {
            disc.insert((
                Falling {
                    delay,
                    start: board_top(size) + 1.0,
                    speed,
                    velocity: 0.0,
                    rest: cell_y(row),
                },
                Transform::from_xyz(cell_x(size, col), if delay > 0.0 { -200.0 } else { board_top(size) + 1.0 }, 0.0)
                    .with_rotation(rotation),
                Visibility::Inherited,
            ));
        }
        None => {
            disc.remove::<Falling>()
                .insert((Transform::from_xyz(cell_x(size, col), cell_y(row), 0.0).with_rotation(rotation), Visibility::Inherited));
        }
    }
}

fn disc_mesh_for(assets: &SceneAssets, detailed: bool) -> Mesh3d {
    Mesh3d(if detailed { assets.disc.clone() } else { assets.disc_simple.clone() })
}

/// Mesh and material for a newly spawned pool disc.
pub fn new_disc_look(assets: &SceneAssets, player: Player, detailed: bool) -> (Mesh3d, MeshMaterial3d<StandardMaterial>) {
    (disc_mesh_for(assets, detailed), MeshMaterial3d(assets.solid[index(player)].clone()))
}

/// Gives a reused disc the right mesh and colour, touching them only if they differ.
pub fn dress_disc(assets: &SceneAssets, detailed: bool, mesh: &mut Mut<Mesh3d>) {
    mesh.set_if_neq(disc_mesh_for(assets, detailed));
}

/// The win glow for one disc: a slightly bigger glowing copy over it, as its child, so
/// it pops with the disc. Spawned whole; despawned when the table starts again.
pub fn halo(commands: &mut Commands, assets: &SceneAssets, disc: Entity, player: Player, detailed: bool) -> Entity {
    commands
        .spawn((
            disc_mesh_for(assets, detailed),
            MeshMaterial3d(assets.glowing[index(player)].clone()),
            Transform::from_scale(Vec3::new(1.05, 1.1, 1.05)),
            ChildOf(disc),
        ))
        .id()
}


fn fall(
    mut commands: Commands,
    time: Res<Time>,
    mut discs: Query<(Entity, &mut Falling, &mut Transform, &mut Visibility, &ChildOf)>,
    tables: Query<&Built>,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (entity, mut falling, mut transform, mut visibility, parent) in &mut discs {
        if falling.delay > 0.0 {
            falling.delay -= dt;
            if falling.delay <= 0.0 {
                transform.translation.y = falling.start;
            }
            continue;
        }
        let dt = dt * falling.speed;
        *visibility = Visibility::Inherited;
        falling.velocity -= GRAVITY * dt;
        transform.translation.y += falling.velocity * dt;
        if transform.translation.y <= falling.rest {
            transform.translation.y = falling.rest;
            // First impact: rock the board a little, harder for a longer drop.
            if falling.velocity < -4.0
                && let Ok(built) = tables.get(parent.parent())
            {
                let strength = (-falling.velocity / 12.0).min(1.5);
                commands.entity(built.root).insert(Thud { age: 0.0, strength });
            }
            if falling.velocity.abs() * BOUNCE > SETTLE_SPEED {
                falling.velocity = -falling.velocity * BOUNCE;
            } else {
                commands.entity(entity).remove::<Falling>();
            }
        }
    }
}

/// The "thinking" disc: only with a single board (with several it would be noise).
fn glide_tables(time: Res<Time>, mut tables: Query<(&Spot, &mut Transform)>) {
    let blend = 1.0 - (-time.delta_secs() * 5.0).exp();
    for (spot, mut transform) in &mut tables {
        transform.translation = transform.translation.lerp(spot.0, blend);
    }
}

fn thud(mut commands: Commands, time: Res<Time>, mut boards: Query<(Entity, &mut Thud, &mut Transform)>) {
    for (entity, mut thud, mut transform) in &mut boards {
        thud.age += time.delta_secs();
        let swing = (thud.age * 32.0).sin() * (-thud.age * 9.0).exp() * 0.018 * thud.strength;
        transform.rotation = Quat::from_rotation_x(-swing.abs());
        if thud.age > 0.6 {
            transform.rotation = Quat::IDENTITY;
            commands.entity(entity).remove::<Thud>();
        }
    }
}

fn hover_ghost(
    games: Query<&Game>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut ghosts: Query<(&Ghost, &mut Transform, &mut Visibility)>,
) {
    let Some(game) = games.iter().find(|game| game.table == 0) else {
        return;
    };
    let t = time.elapsed_secs();
    let thinking = settings.boards == 1
        && game.waiting
        && settings.animated()
        && matches!(game.phase, Phase::Running | Phase::Paused);
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
    // Quantised so the material changes only a few dozen times a second at most.
    let pulse = ((1.2 + (time.elapsed_secs() * 5.0).sin()) * 20.0).round() / 20.0;
    for (handle, colour) in assets.glowing.iter().zip([RED, YELLOW]) {
        let emissive = colour.to_linear() * pulse;
        let stale = materials.get(handle).is_some_and(|material| material.emissive != emissive);
        if stale && let Some(mut material) = materials.get_mut(handle) {
            material.emissive = emissive;
        }
    }
}

fn orbit_camera(
    time: Res<Time>,
    settings: Res<Settings>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    margins: Res<UiMargins>,
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

    // Fit the boards into the part of the window the UI does not cover. With rows of
    // boards the camera looks down more so the back rows show over the front ones.
    let size = settings.board_size();
    let (columns, rows) = grid(settings.boards);
    let wall = stacked(settings.boards);
    // A wall faces the camera; rows going back need it to look down over them.
    let back_rows = if wall { 0.0 } else { usize_f32(rows) - 1.0 };
    let lift = back_rows * 0.32;
    let width_px = window.width().max(1.0);
    let height_px = window.height().max(1.0);
    let free_w = (width_px - margins.left - margins.right).max(width_px * 0.3);
    let free_h = (height_px - margins.top - margins.bottom).max(height_px * 0.3);
    let half_fov = (FRAC_PI_4 / 2.0).tan();
    let step_x = to_f32(size.columns) + MARGIN * 2.0 + TABLE_GAP;
    // Perspective makes the front row look wider than the back, so leave room for it.
    let board_w = (usize_f32(columns) * step_x - TABLE_GAP + 0.6) * (1.0 + back_rows * 0.14);
    let board_h = if wall {
        usize_f32(rows) * wall_step(size) - TABLE_GAP + 1.0
    } else {
        board_top(size) + 1.6 + back_rows * ROW_DEPTH * 0.8
    };
    let fit = (board_w * height_px / (2.0 * half_fov * free_w)).max(board_h * height_px / (2.0 * half_fov * free_h)) * 1.08;
    let wanted = fit * orbit.zoom;
    let blend = 1.0 - (-time.delta_secs() * 4.0).exp();
    orbit.distance += (wanted - orbit.distance) * blend;
    let world_per_px = 2.0 * orbit.distance * half_fov / height_px;
    // Centre the boards in the free area, not the window.
    let shift_y = (margins.bottom - margins.top) / 2.0 * world_per_px;
    let shift_x = (margins.right - margins.left) / 2.0 * world_per_px;
    let wall_centre = if wall { (usize_f32(rows) - 1.0) * wall_step(size) / 2.0 } else { 0.0 };
    let target = board_centre(size) + Vec3::new(shift_x, 0.5 - shift_y + wall_centre, 0.0);
    orbit.target = orbit.target.lerp(target, blend);
    orbit.lift += (lift - orbit.lift) * blend;

    let sway = (time.elapsed_secs() * 0.25).sin() * 0.12;
    let yaw = orbit.yaw + sway;
    let pitch = (orbit.pitch + orbit.lift).min(1.3);
    let offset = Vec3::new(
        orbit.distance * pitch.cos() * yaw.sin(),
        orbit.distance * pitch.sin(),
        orbit.distance * pitch.cos() * yaw.cos(),
    );
    *transform = Transform::from_translation(orbit.target + offset).looking_at(orbit.target, Vec3::Y);
}
