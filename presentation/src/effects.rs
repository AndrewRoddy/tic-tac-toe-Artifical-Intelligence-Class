//! Win effects: when a game is won the winning line lights up with a beam that draws
//! itself through the four discs, the discs pop forward, confetti bursts out in the
//! winner's colours and a coloured light flashes. Only at animated speeds (0.1 s per
//! move and up); in fast mode the glow alone marks the win.
//!
//! Everything spawned here is a child of the table and is cleared when that table
//! starts its next game.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use c4::random::SplitMix;

use crate::game::{Game, GameFlow, Phase, Player, Settings};
use crate::scene::{Disc, Falling, board_top, cell_x, cell_y};
use crate::sizes::BoardSize;
use crate::theme::{ACCENT, RED, TEXT, YELLOW};

/// Sent by the game when a table's match ends in a win.
#[derive(Message, Clone)]
pub struct Won {
    pub table: Entity,
    pub index: usize,
    pub player: Player,
    pub size: BoardSize,
    /// The winning cells (four or more, possibly in two directions).
    pub line: Vec<(u32, u32)>,
}

/// Anything spawned for a win, removed when its table starts again.
#[derive(Component)]
struct Effect(usize);

/// The beam through the winning line: grows from one end to the other, then breathes.
#[derive(Component)]
struct Beam {
    age: f32,
    length: f32,
}

/// A winning disc's pop: forward and bigger, then settles slightly forward.
#[derive(Component)]
pub struct WinPop {
    age: f32,
    rest: Vec3,
}

#[derive(Component)]
struct Confetti {
    velocity: Vec3,
    spin: Vec3,
    age: f32,
    life: f32,
}

#[derive(Component)]
struct Flash {
    age: f32,
}

#[derive(Resource)]
struct EffectAssets {
    beam: [Handle<StandardMaterial>; 2],
    confetti: Vec<Handle<StandardMaterial>>,
    beam_mesh: Handle<Mesh>,
    bits: [Handle<Mesh>; 2],
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Won>()
            .add_systems(Startup, setup)
            .add_systems(Update, (celebrate, clean_up, beams, pops, confetti, flashes).chain().after(GameFlow));
    }
}

fn glowing(materials: &mut Assets<StandardMaterial>, colour: Color, glow: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: colour,
        emissive: colour.to_linear() * glow,
        perceptual_roughness: 0.3,
        ..default()
    })
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let assets = EffectAssets {
        beam: [glowing(&mut materials, RED, 6.0), glowing(&mut materials, YELLOW, 6.0)],
        confetti: [RED, YELLOW, ACCENT, TEXT]
            .into_iter()
            .map(|colour| glowing(&mut materials, colour, 1.5))
            .collect(),
        beam_mesh: meshes.add(Capsule3d::new(0.11, 1.0)),
        bits: [meshes.add(Cuboid::new(0.16, 0.16, 0.03)), meshes.add(Cylinder::new(0.09, 0.03))],
    };
    commands.insert_resource(assets);
}

fn cell_centre(size: BoardSize, (col, row): (u32, u32)) -> Vec3 {
    Vec3::new(cell_x(size, col), cell_y(row), 0.0)
}

/// The two farthest-apart cells of the line: the beam's ends.
fn ends(size: BoardSize, line: &[(u32, u32)]) -> Option<(Vec3, Vec3)> {
    let points: Vec<Vec3> = line.iter().map(|&cell| cell_centre(size, cell)).collect();
    let mut best: Option<(Vec3, Vec3)> = None;
    for &a in &points {
        for &b in &points {
            if best.is_none_or(|(x, y)| a.distance(b) > x.distance(y)) {
                best = Some((a, b));
            }
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn celebrate(
    mut commands: Commands,
    mut won: MessageReader<Won>,
    settings: Res<Settings>,
    assets: Res<EffectAssets>,
    time: Res<Time>,
    discs: Query<(Entity, &Disc, &Transform)>,
) {
    for win in won.read() {
        if !settings.animated() {
            continue;
        }
        let seat = win.player.seat();
        let Some((from, to)) = ends(win.size, &win.line) else {
            continue;
        };
        // Beam through the line, in front of the discs.
        let length = from.distance(to);
        let direction = (to - from).normalize_or(Vec3::X);
        let rotation = Quat::from_rotation_arc(Vec3::Y, direction);
        commands.spawn((
            Effect(win.index),
            Beam { age: 0.0, length },
            Mesh3d(assets.beam_mesh.clone()),
            MeshMaterial3d(assets.beam[seat].clone()),
            Transform::from_translation(from + Vec3::Z * 0.45).with_rotation(rotation).with_scale(Vec3::new(1.0, 0.001, 1.0)),
            ChildOf(win.table),
        ));
        // The winning discs pop forward.
        for (entity, disc, transform) in &discs {
            if disc.table == win.index && win.line.contains(&(disc.col, disc.row)) {
                let rest = Vec3::new(transform.translation.x, cell_y(disc.row), transform.translation.z);
                commands.entity(entity).remove::<Falling>().insert((WinPop { age: 0.0, rest }, Visibility::Inherited));
            }
        }
        // A flash of the winner's colour.
        let centre = (from + to) / 2.0;
        let colour = if seat == 0 { RED } else { YELLOW };
        commands.spawn((
            Effect(win.index),
            Flash { age: 0.0 },
            PointLight {
                color: colour,
                intensity: 0.0,
                range: 20.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(centre + Vec3::new(0.0, 0.0, 2.5)),
            ChildOf(win.table),
        ));
        // Confetti: fewer when many boards are going at once.
        let count = if settings.boards > 3 { 24 } else { 70 };
        let seed = time.elapsed().as_nanos() as u64 ^ (u64::try_from(win.index).unwrap_or(0) << 32);
        let mut rng = SplitMix::new(seed);
        let top = board_top(win.size);
        for piece in 0..count {
            let palette = if piece % 3 == 0 { 2 + piece % 2 } else { seat };
            let start = centre + Vec3::new(rng.symmetric(0.6), rng.symmetric(0.4), 0.6);
            let velocity = Vec3::new(rng.symmetric(5.0), 6.5 + rng.symmetric(3.0) + top * 0.15, 2.5 + rng.symmetric(2.0));
            commands.spawn((
                Effect(win.index),
                Confetti {
                    velocity,
                    spin: Vec3::new(rng.symmetric(12.0), rng.symmetric(12.0), rng.symmetric(12.0)),
                    age: 0.0,
                    life: 1.8 + rng.symmetric(0.5),
                },
                Mesh3d(assets.bits[piece % 2].clone()),
                MeshMaterial3d(assets.confetti[palette].clone()),
                Transform::from_translation(start).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
                ChildOf(win.table),
            ));
        }
    }
}

/// Removes a table's effects once it is no longer showing its result.
fn clean_up(
    mut commands: Commands,
    games: Query<&Game>,
    effects: Query<(Entity, &Effect)>,
    mut pops: Query<(Entity, &Disc, &WinPop, &mut Transform)>,
) {
    let finished = |index: usize| games.iter().any(|game| game.table == index && game.phase == Phase::Finished);
    for (entity, effect) in &effects {
        if !finished(effect.0) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, disc, pop, mut transform) in &mut pops {
        if !finished(disc.table) {
            transform.translation = pop.rest;
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<WinPop>();
        }
    }
}

fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u.powi(3) + c1 * u.powi(2)
}

fn beams(time: Res<Time>, mut beams: Query<(&mut Beam, &mut Transform)>) {
    for (mut beam, mut transform) in &mut beams {
        beam.age += time.delta_secs();
        let grow = (beam.age / 0.35).clamp(0.0, 1.0);
        let length = beam.length * (1.0 - (1.0 - grow).powi(3));
        // The capsule is 1 long on y and centred, so scale it and move its centre along.
        let direction = transform.rotation * Vec3::Y;
        let start = transform.translation - direction * transform.scale.y / 2.0;
        let breathe = 1.0 + (beam.age * 6.0).sin() * 0.15 * grow;
        transform.scale = Vec3::new(breathe, length.max(0.001), breathe);
        transform.translation = start + direction * transform.scale.y / 2.0;
    }
}

fn pops(time: Res<Time>, mut discs: Query<(&mut WinPop, &mut Transform)>) {
    for (mut pop, mut transform) in &mut discs {
        pop.age += time.delta_secs();
        let t = (pop.age / 0.45).clamp(0.0, 1.0);
        let out = ease_out_back(t);
        let wobble = (pop.age * 7.0).sin() * 0.04 * (1.0 - t);
        transform.translation = pop.rest + Vec3::new(0.0, 0.0, 0.32 * out);
        transform.scale = Vec3::splat(1.0 + 0.22 * out + wobble);
    }
}

fn confetti(mut commands: Commands, time: Res<Time>, mut pieces: Query<(Entity, &mut Confetti, &mut Transform)>) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (entity, mut piece, mut transform) in &mut pieces {
        piece.age += dt;
        if piece.age >= piece.life {
            commands.entity(entity).despawn();
            continue;
        }
        piece.velocity.y -= 14.0 * dt;
        piece.velocity *= 1.0 - 0.9 * dt;
        transform.translation += piece.velocity * dt;
        let spin = piece.spin * dt;
        transform.rotate(Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z));
        let fade = ((piece.life - piece.age) / 0.4).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(fade);
    }
}

fn flashes(time: Res<Time>, mut lights: Query<(&mut Flash, &mut PointLight)>) {
    for (mut flash, mut light) in &mut lights {
        flash.age += time.delta_secs();
        let rise = (flash.age / 0.08).clamp(0.0, 1.0);
        let fall = (-(flash.age - 0.08).max(0.0) * 3.0).exp();
        light.intensity = 900_000.0 * rise * fall;
    }
}
