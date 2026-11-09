//! Presentation backdrop: something is always moving behind the slides.
//!
//! - A 7x6 board floats behind the content and plays a real game, MCTS against the
//!   learned alpha-beta through the same engine thread the arena uses. When a game
//!   ends the winning line glows, then the board empties and starts again. It glides
//!   to a new place on every slide.
//! - Discs drift slowly through the far background.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use c4::random::SplitMix;
use c4::tournament::Entrant;

use crate::engine::{Config, Engine, Reply, Think};
use crate::present::{Deck, Mode, STAGE};
use crate::scene::{SceneAssets, board_parts, board_top, cell_x, cell_y};
use crate::sizes::{BoardSize, LADDER};
use crate::slides::{SLIDES, Visual};
use crate::theme::{BG, BOARD, RED, YELLOW};

const SIZE: BoardSize = LADDER[3];
const MOVE_SECONDS: f32 = 0.35;
const HOLD_SECONDS: f32 = 1.6;

/// Where the backdrop board sits for a slide (relative to the stage): position, turn, scale.
/// Far back and low, so it frames the slide instead of crossing the text.
const POSES: [(Vec3, f32, f32); 4] = [
    (Vec3::new(15.0, -15.0, -28.0), -0.45, 1.7),
    (Vec3::new(-17.0, -16.0, -30.0), 0.5, 1.8),
    (Vec3::new(1.0, -19.0, -34.0), -0.15, 2.1),
    (Vec3::new(21.0, -14.0, -32.0), -0.7, 1.8),
];

#[derive(Component)]
struct Backdrop;

/// The floating board's root.
#[derive(Component)]
struct Mascot;

#[derive(Component)]
struct MascotDisc {
    col: u32,
    row: u32,
    velocity: f32,
    falling: bool,
}

#[derive(Component)]
struct Drifter {
    velocity: Vec3,
    spin: Vec3,
}

/// The backdrop game: its engine, the column heights, and when it last changed.
#[derive(Resource)]
struct MascotGame {
    engine: Option<Engine>,
    heights: [u32; 9],
    moves: u32,
    waiting: bool,
    next_at: f32,
    finished: Option<f32>,
    line: Vec<(u32, u32)>,
    grid: Vec<Option<usize>>,
    seed: u64,
}

#[derive(Resource)]
struct BackdropMaterials {
    board: Handle<StandardMaterial>,
    discs: [Handle<StandardMaterial>; 2],
    glow: [Handle<StandardMaterial>; 2],
    drift: [Handle<StandardMaterial>; 2],
}

pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(OnEnter(Mode::Slides), spawn)
            .add_systems(OnExit(Mode::Slides), despawn)
            .add_systems(Update, (play, drop_discs, glide, drift).run_if(in_state(Mode::Slides)));
    }
}

/// A see-through material: the backdrop should never compete with the slide.
fn dim(materials: &mut Assets<StandardMaterial>, colour: Color, alpha: f32, glow: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: colour.mix(&BG, 0.2).with_alpha(alpha),
        emissive: colour.to_linear() * glow,
        perceptual_roughness: 0.4,
        alpha_mode: AlphaMode::Blend,
        ..default()
    })
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(BackdropMaterials {
        board: dim(&mut materials, BOARD, 0.42, 0.0),
        discs: [dim(&mut materials, RED, 0.5, 0.05), dim(&mut materials, YELLOW, 0.5, 0.05)],
        glow: [dim(&mut materials, RED, 0.9, 2.5), dim(&mut materials, YELLOW, 0.9, 2.5)],
        drift: [dim(&mut materials, RED, 0.16, 0.0), dim(&mut materials, YELLOW, 0.16, 0.0)],
    });
}

fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(7, |elapsed| u64::try_from(elapsed.as_nanos() & u128::from(u64::MAX)).unwrap_or(7))
}

fn new_engine(seed: u64) -> Engine {
    Engine::spawn(Config {
        size: SIZE,
        red: Entrant::Mcts,
        yellow: Entrant::Learned,
        opening_stones: 4,
        seed,
    })
}

fn pose(index: usize) -> (Vec3, Quat, f32) {
    // Slides that show their own board get the backdrop out of the way, far left.
    if matches!(SLIDES.get(index).map(|slide| &slide.visual), Some(Visual::Board { .. })) {
        return (STAGE + Vec3::new(-30.0, -15.0, -34.0), Quat::from_rotation_y(0.6), 1.7);
    }
    let (at, turn, scale) = POSES[index % POSES.len()];
    (STAGE + at, Quat::from_rotation_y(turn), scale)
}

fn spawn(
    mut commands: Commands,
    deck: Res<Deck>,
    materials: Res<BackdropMaterials>,
    assets: Res<SceneAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let seed = clock_seed();
    commands.insert_resource(MascotGame {
        engine: Some(new_engine(seed)),
        heights: [0; 9],
        moves: 0,
        waiting: false,
        next_at: 0.0,
        finished: None,
        line: Vec::new(),
        grid: vec![None; 42],
        seed,
    });
    let (at, turn, scale) = pose(deck.index);
    let parts = board_parts(SIZE, &mut meshes, &materials.board);
    commands
        .spawn((
            Backdrop,
            Mascot,
            Transform::from_translation(at).with_rotation(turn).with_scale(Vec3::splat(scale)),
            // Hidden: the slides' own visuals are centre stage now, and a board drifting
            // behind them was distracting. Only the far drifting discs remain.
            Visibility::Hidden,
        ))
        .with_children(|board| {
            for (mesh, _, transform) in parts {
                board.spawn((mesh, MeshMaterial3d(materials.board.clone()), transform));
            }
        });
    // Drifting discs far behind everything.
    let mut rng = SplitMix::new(seed ^ 0xD15C);
    for index in 0..26_usize {
        let position = STAGE + Vec3::new(rng.symmetric(55.0), rng.symmetric(32.0), -45.0 + rng.symmetric(8.0));
        commands.spawn((
            Backdrop,
            Drifter {
                velocity: Vec3::new(rng.symmetric(0.4), -0.6 - rng.symmetric(0.3).abs(), 0.0),
                spin: Vec3::new(rng.symmetric(0.8), rng.symmetric(0.8), 0.0),
            },
            Mesh3d(assets.disc.clone()),
            MeshMaterial3d(materials.drift[index % 2].clone()),
            Transform::from_translation(position)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, FRAC_PI_2 + rng.symmetric(1.0), rng.symmetric(TAU), 0.0))
                .with_scale(Vec3::splat(2.2 + rng.symmetric(0.8))),
        ));
    }
}

fn despawn(mut commands: Commands, items: Query<Entity, With<Backdrop>>) {
    for entity in &items {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<MascotGame>();
}

/// Runs the backdrop game: one move at a time, then a pause on the result, then again.
fn play(
    mut commands: Commands,
    time: Res<Time>,
    game: Option<ResMut<MascotGame>>,
    materials: Res<BackdropMaterials>,
    assets: Res<SceneAssets>,
    mascot: Query<Entity, With<Mascot>>,
    mut discs: Query<(Entity, &MascotDisc, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let (Some(mut game), Ok(board)) = (game, mascot.single()) else {
        return;
    };
    let now = time.elapsed_secs();
    if let Some(ended) = game.finished {
        if now - ended < HOLD_SECONDS {
            return;
        }
        for (entity, ..) in &discs {
            commands.entity(entity).despawn();
        }
        game.seed = game.seed.wrapping_add(0x9E37_79B9);
        let seed = game.seed;
        *game = MascotGame {
            engine: Some(new_engine(seed)),
            heights: [0; 9],
            moves: 0,
            waiting: false,
            next_at: now + 0.3,
            finished: None,
            line: Vec::new(),
            grid: vec![None; 42],
            seed,
        };
        return;
    }
    let mut placed = Vec::new();
    while let Some(reply) = game.engine.as_ref().and_then(Engine::poll) {
        match reply {
            Reply::Ready { opening } => placed.extend(opening.into_iter().map(|col| (col, false, false))),
            Reply::Moved { col, wins, full, .. } => {
                game.waiting = false;
                placed.push((col, wins, full));
            }
            Reply::Failed(_) => {
                game.finished = Some(now);
                game.engine = None;
            }
        }
    }
    for (col, wins, full) in placed {
        let Some(height) = game.heights.get(usize::try_from(col).unwrap_or(9)).copied() else {
            continue;
        };
        if height >= SIZE.rows {
            continue;
        }
        let player = usize::try_from(game.moves % 2).unwrap_or(0);
        game.heights[usize::try_from(col).unwrap_or(0)] += 1;
        game.moves += 1;
        let index = usize::try_from(height * SIZE.columns + col).unwrap_or(0);
        if let Some(cell) = game.grid.get_mut(index) {
            *cell = Some(player);
        }
        commands.spawn((
            Backdrop,
            MascotDisc {
                col,
                row: height,
                velocity: 0.0,
                falling: true,
            },
            Mesh3d(assets.disc.clone()),
            MeshMaterial3d(materials.discs[player].clone()),
            Transform::from_xyz(cell_x(SIZE, col), board_top(SIZE) + 1.2, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ChildOf(board),
        ));
        if wins || full {
            game.finished = Some(now + 0.5);
            game.engine = None;
            if wins {
                game.line = winning_line(&game.grid, col, height);
                let line = game.line.clone();
                for (_, disc, mut material) in &mut discs {
                    if line.contains(&(disc.col, disc.row)) {
                        material.0 = materials.glow[player].clone();
                    }
                }
                // The winning disc itself was spawned this frame.
                let glow = materials.glow[player].clone();
                commands.queue(move |world: &mut World| {
                    let mut query = world.query::<(&MascotDisc, &mut MeshMaterial3d<StandardMaterial>)>();
                    for (disc, mut material) in query.iter_mut(world) {
                        if line.contains(&(disc.col, disc.row)) {
                            material.0 = glow.clone();
                        }
                    }
                });
            }
        }
    }
    if game.finished.is_none() && !game.waiting && now >= game.next_at {
        if let Some(engine) = game.engine.as_ref() {
            engine.request(Think {
                per_move: Duration::from_secs_f32(MOVE_SECONDS),
                count: 1,
            });
            game.waiting = true;
            game.next_at = now + MOVE_SECONDS + 0.35;
        }
    }
}

/// Cells in a line of four or more through (col, row) on the backdrop board.
fn winning_line(grid: &[Option<usize>], col: u32, row: u32) -> Vec<(u32, u32)> {
    let at = |c: i64, r: i64| -> Option<usize> {
        let (c, r) = (u32::try_from(c).ok()?, u32::try_from(r).ok()?);
        if c >= SIZE.columns || r >= SIZE.rows {
            return None;
        }
        *grid.get(usize::try_from(r * SIZE.columns + c).ok()?)?
    };
    let Some(player) = at(i64::from(col), i64::from(row)) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (dc, dr) in [(1_i64, 0_i64), (0, 1), (1, 1), (1, -1)] {
        let mut line = vec![(col, row)];
        for sign in [1_i64, -1] {
            let (mut c, mut r) = (i64::from(col), i64::from(row));
            loop {
                c += dc * sign;
                r += dr * sign;
                if at(c, r) != Some(player) {
                    break;
                }
                line.push((u32::try_from(c).unwrap_or(0), u32::try_from(r).unwrap_or(0)));
            }
        }
        if line.len() >= 4 {
            found.extend(line);
        }
    }
    found
}

fn drop_discs(time: Res<Time>, mut discs: Query<(&mut MascotDisc, &mut Transform)>) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (mut disc, mut transform) in &mut discs {
        if !disc.falling {
            continue;
        }
        disc.velocity -= 30.0 * dt;
        transform.translation.y += disc.velocity * dt;
        let rest = cell_y(disc.row);
        if transform.translation.y <= rest {
            transform.translation.y = rest;
            if disc.velocity.abs() * 0.3 > 1.0 {
                disc.velocity = -disc.velocity * 0.3;
            } else {
                disc.falling = false;
            }
        }
    }
}

/// Eases the backdrop board toward this slide's pose, with a slow float on top.
fn glide(time: Res<Time>, deck: Res<Deck>, mut boards: Query<&mut Transform, With<Mascot>>) {
    let (at, turn, scale) = pose(deck.index);
    let t = time.elapsed_secs();
    let blend = 1.0 - (-time.delta_secs() * 1.8).exp();
    for mut transform in &mut boards {
        let bob = Vec3::new((t * 0.31).sin() * 0.8, (t * 0.47).sin() * 0.5, 0.0);
        transform.translation = transform.translation.lerp(at + bob, blend);
        let sway = Quat::from_rotation_y((t * 0.23).sin() * 0.18);
        transform.rotation = transform.rotation.slerp(turn * sway, blend);
        transform.scale = transform.scale.lerp(Vec3::splat(scale), blend);
    }
}

fn drift(time: Res<Time>, mut discs: Query<(&Drifter, &mut Transform)>) {
    let dt = time.delta_secs();
    for (drifter, mut transform) in &mut discs {
        transform.translation += drifter.velocity * dt;
        transform.rotate(Quat::from_euler(EulerRot::XYZ, drifter.spin.x * dt, drifter.spin.y * dt, 0.0));
        // Wrap round: leave at the bottom, come back in at the top.
        if transform.translation.y < STAGE.y - 34.0 {
            transform.translation.y = STAGE.y + 34.0;
        }
    }
}
