//! Presentation mode: a slide deck where everything, down to the text, is 3D.
//!
//! The slides live on a "stage" far from the arena table. Entering the mode hides the
//! 2D UI and flies the camera to the stage; Esc flies back. Right / Space / click go
//! forward, Left goes back. Content is in `slides.rs`.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bevy::prelude::*;
use c4::outcome::Outcome;

use crate::roster::ROSTER;
use crate::scene::{SceneAssets, board_parts, board_top, cell_x, cell_y};
use crate::sizes::BoardSize;
use crate::slides::{Bars, SLIDES, Slide, Visual, solve_position, solved_columns};
use crate::text3d::{Align, Face, Fonts, Spec, text_bundle};
use crate::theme::{ACCENT, AQUA, BOARD, FAINT, GREEN, MUTED, RAISED, RED, TEXT, YELLOW};

/// Where the slides are built, well away from the arena table.
pub const STAGE: Vec3 = Vec3::new(0.0, 0.0, -140.0);
/// Camera distance from the stage plane; the layout below is sized for 16:9 at this distance.
const VIEW_DISTANCE: f32 = 24.0;
const LEFT: f32 = -16.5;
/// Visible half-width the layout needs (world units).
const HALF_WIDTH: f32 = 17.8;
const HALF_HEIGHT: f32 = 9.8;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    Arena,
    Slides,
}

/// Which slide is showing, and whether it needs to be (re)built.
#[derive(Resource, Default)]
pub struct Deck {
    pub index: usize,
    dirty: bool,
    /// +1 going forward, -1 going back: which way slides fly in and out.
    direction: f32,
    /// The live-solved position: `None` while the solver is still running.
    solved: Arc<Mutex<Option<Option<Vec<Option<Outcome>>>>>>,
    waiting_for_solve: bool,
}

impl Deck {
    pub fn go_to(&mut self, index: usize) {
        let index = index.min(SLIDES.len() - 1);
        self.direction = if index < self.index { -1.0 } else { 1.0 };
        self.index = index;
        self.dirty = true;
    }
}

#[derive(Resource)]
struct Palette {
    title: Handle<StandardMaterial>,
    body: Handle<StandardMaterial>,
    faint: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    bar: Handle<StandardMaterial>,
    tile: Handle<StandardMaterial>,
    layer: Handle<StandardMaterial>,
    families: [Handle<StandardMaterial>; 5],
    outcomes: [Handle<StandardMaterial>; 3],
}

/// Everything on the current slide; despawned when the slide changes.
#[derive(Component)]
struct SlideItem;

/// Pops in after `delay`: scales up from nothing with a small overshoot and sweeps
/// into place from the side the deck is moving toward.
#[derive(Component)]
struct Appear {
    delay: f32,
    age: f32,
    home: Vec3,
    side: f32,
}

/// The previous slide's pieces on their way out.
#[derive(Component)]
struct Leaving {
    age: f32,
    from: Vec3,
    scale: Vec3,
    side: f32,
}

/// A bar that grows to `length` along x from `start`.
#[derive(Component)]
struct Grow {
    start: f32,
    length: f32,
    delay: f32,
    age: f32,
}

/// A disc that falls into its cell.
#[derive(Component)]
struct Drop {
    rest: f32,
    delay: f32,
    velocity: f32,
}

/// Slow back-and-forth turn about y.
#[derive(Component)]
struct Sway {
    amount: f32,
    speed: f32,
    phase: f32,
}

/// The stage's key light: it flashes when the slide changes.
#[derive(Component)]
struct StageLight;

const STAGE_LIGHT: f32 = 2_500_000.0;

/// Seconds since the slide last changed: drives the camera push-in and light flash.
#[derive(Resource)]
struct Kick(f32);

impl Default for Kick {
    fn default() -> Self {
        Self(10.0)
    }
}

/// The network layers breathe in a wave.
#[derive(Component)]
struct Pulse(f32);

pub struct PresentPlugin;

impl Plugin for PresentPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Mode>()
            .init_resource::<Fonts>()
            .init_resource::<Deck>()
            .init_resource::<Kick>()
            .add_systems(Startup, (setup, start_solver))
            .add_systems(OnEnter(Mode::Slides), enter)
            .add_systems(OnExit(Mode::Slides), leave)
            .add_systems(
                Update,
                (navigate, poll_solver, build_slide, slide_camera, appear, leave_slide, grow, drop_discs, sway, pulse)
                    .chain()
                    .run_if(in_state(Mode::Slides)),
            );
    }
}

fn material(materials: &mut Assets<StandardMaterial>, colour: Color, glow: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: colour,
        emissive: colour.to_linear() * glow,
        perceptual_roughness: 0.45,
        double_sided: true,
        cull_mode: None,
        ..default()
    })
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut make = |colour: Color, glow: f32| material(&mut materials, colour, glow);
    let palette = Palette {
        title: make(TEXT, 0.35),
        body: make(MUTED, 0.3),
        faint: make(FAINT, 0.3),
        accent: make(ACCENT, 0.45),
        bar: make(MUTED.mix(&RAISED, 0.55), 0.1),
        tile: make(RAISED, 0.05),
        layer: make(BOARD.mix(&AQUA, 0.35), 0.15),
        families: [make(ACCENT, 0.4), make(YELLOW, 0.4), make(AQUA, 0.4), make(GREEN, 0.4), make(FAINT, 0.3)],
        outcomes: [make(RED, 0.5), make(YELLOW, 0.5), make(GREEN, 0.5)],
    };
    commands.insert_resource(palette);
    // A warm light over the stage, on top of the scene's sun.
    commands.spawn((
        StageLight,
        PointLight {
            intensity: STAGE_LIGHT,
            range: 80.0,
            color: Color::srgb(1.0, 0.86, 0.7),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(STAGE + Vec3::new(-6.0, 10.0, 14.0)),
    ));
}

/// Solves the live position in the background so it is ready by the time it is shown.
fn start_solver(deck: Res<Deck>) {
    let slot = Arc::clone(&deck.solved);
    thread::Builder::new()
        .name("c4-slide-solver".to_owned())
        .spawn(move || {
            let result = solve_position(Duration::from_secs(120));
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(result);
            }
        })
        .ok();
}

fn enter(mut deck: ResMut<Deck>) {
    deck.dirty = true;
}

fn leave(mut commands: Commands, items: Query<Entity, With<SlideItem>>) {
    for entity in &items {
        commands.entity(entity).despawn();
    }
}

fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut deck: ResMut<Deck>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let next = [KeyCode::ArrowRight, KeyCode::Space, KeyCode::PageDown, KeyCode::Enter, KeyCode::ArrowDown]
        .iter()
        .any(|&key| keys.just_pressed(key))
        || mouse.just_pressed(MouseButton::Left);
    let back = [KeyCode::ArrowLeft, KeyCode::PageUp, KeyCode::Backspace, KeyCode::ArrowUp]
        .iter()
        .any(|&key| keys.just_pressed(key));
    if keys.just_pressed(KeyCode::Escape) {
        mode.set(Mode::Arena);
    } else if keys.just_pressed(KeyCode::Home) {
        deck.go_to(0);
    } else if next {
        if deck.index + 1 >= SLIDES.len() {
            mode.set(Mode::Arena);
        } else {
            let index = deck.index + 1;
            deck.go_to(index);
        }
    } else if back && deck.index > 0 {
        let index = deck.index - 1;
        deck.go_to(index);
    }
}

fn poll_solver(mut deck: ResMut<Deck>) {
    if deck.waiting_for_solve && deck.solved.lock().is_ok_and(|slot| slot.is_some()) {
        deck.waiting_for_solve = false;
        deck.dirty = true;
    }
}

/// Builds slides: one spawner per frame of work keeps the borrow juggling simple.
struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    fonts: &'a mut Fonts,
    meshes: &'a mut Assets<Mesh>,
    palette: &'a Palette,
    assets: &'a SceneAssets,
    /// Which side new pieces sweep in from.
    side: f32,
}

impl Builder<'_, '_, '_> {
    /// Spawns a line of text that pops in after `delay`. Silently skipped without fonts.
    fn text(&mut self, spec: &Spec<'_>, material: &Handle<StandardMaterial>, at: Vec2, delay: f32) {
        if spec.text.is_empty() {
            return;
        }
        let home = STAGE + at.extend(0.0);
        let Some(bundle) = text_bundle(self.fonts, self.meshes, spec, material.clone(), Vec3::ZERO) else {
            return;
        };
        self.commands
            .spawn((SlideItem, appear_at(home, delay, self.side), Transform::from_translation(home).with_scale(Vec3::splat(0.0001)), Visibility::default()))
            .with_children(|item| {
                item.spawn(bundle);
            });
    }

    fn block(&mut self, mesh: Mesh, material: &Handle<StandardMaterial>, at: Vec3, delay: f32) -> Entity {
        let mesh = self.meshes.add(mesh);
        self.commands
            .spawn((SlideItem, appear_at(STAGE + at, delay, self.side), Mesh3d(mesh), MeshMaterial3d(material.clone())))
            .id()
    }

    /// A board standing at `base` (bottom centre) with the given scale and discs.
    fn board(&mut self, size: BoardSize, base: Vec3, scale: f32, moves: &[u32], delay: f32, sway: bool) -> Entity {
        let parts = board_parts(size, self.meshes, &self.assets.board);
        let mut heights = vec![0_u32; usize::try_from(size.columns).unwrap_or(0)];
        let mut discs = Vec::new();
        for (step, &col) in moves.iter().enumerate() {
            let Some(height) = heights.get_mut(usize::try_from(col).unwrap_or(usize::MAX)) else {
                continue;
            };
            if *height >= size.rows {
                continue;
            }
            let row = *height;
            *height += 1;
            let player = step % 2;
            let wait = delay + 0.35 + f32::from(u16::try_from(step).unwrap_or(0)) * 0.12;
            discs.push((col, row, player, wait));
        }
        let disc_mesh = self.assets.disc.clone();
        let solid = self.assets.solid.clone();
        let top = board_top(size) + 1.5;
        let mut root = self.commands.spawn((
            SlideItem,
            appear_at(STAGE + base, delay, self.side),
            Transform::from_translation(STAGE + base).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ));
        if sway {
            root.insert(Sway {
                amount: 0.35,
                speed: 0.35,
                phase: 0.0,
            });
        }
        root.insert(Scale(scale));
        root.with_children(|board| {
            for part in parts {
                board.spawn(part);
            }
            for (col, row, player, wait) in discs {
                board.spawn((
                    Mesh3d(disc_mesh.clone()),
                    MeshMaterial3d(solid[player].clone()),
                    Transform::from_xyz(cell_x(size, col), top, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
                    Drop {
                        rest: cell_y(row),
                        delay: wait,
                        velocity: 0.0,
                    },
                    Visibility::Hidden,
                ));
            }
        });
        root.id()
    }
}

/// The scale a slide element settles at (boards are not unit size).
#[derive(Component, Clone, Copy)]
struct Scale(f32);

const fn appear_at(home: Vec3, delay: f32, side: f32) -> Appear {
    Appear {
        delay,
        age: 0.0,
        home,
        side,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_slide(
    mut commands: Commands,
    mut deck: ResMut<Deck>,
    mut kick: ResMut<Kick>,
    mut fonts: ResMut<Fonts>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<Palette>,
    assets: Res<SceneAssets>,
    old: Query<(Entity, &Transform), (With<SlideItem>, Without<Leaving>)>,
) {
    if !deck.dirty {
        return;
    }
    deck.dirty = false;
    kick.0 = 0.0;
    // The old slide flies out the way we are going; its pieces stop their own animations.
    for (entity, transform) in &old {
        commands.entity(entity).remove::<(Appear, Grow)>().insert(Leaving {
            age: 0.0,
            from: transform.translation,
            scale: transform.scale,
            side: deck.direction,
        });
    }
    let slide = &SLIDES[deck.index];
    let solved = deck.solved.lock().ok().and_then(|slot| slot.clone());
    deck.waiting_for_solve = matches!(slide.visual, Visual::Solved) && solved.is_none();
    let mut builder = Builder {
        commands: &mut commands,
        fonts: &mut fonts,
        meshes: &mut meshes,
        palette: &palette,
        assets: &assets,
        side: deck.direction,
    };
    frame(&mut builder, slide, deck.index);
    match &slide.visual {
        Visual::Board { columns, rows, moves } => showcase(&mut builder, *columns, *rows, moves),
        Visual::Boards(list) => boards(&mut builder, list),
        Visual::Bars(bars) => bar_chart(&mut builder, bars),
        Visual::Agents => agents(&mut builder),
        Visual::Network => network(&mut builder),
        Visual::Solved => solved_board(&mut builder, solved.flatten()),
    }
}

/// Kicker, title, rule, body lines and the footer: the same on every slide.
fn frame(builder: &mut Builder<'_, '_, '_>, slide: &Slide, index: usize) {
    let palette = builder.palette;
    let (accent, title, body, faint) = (palette.accent.clone(), palette.title.clone(), palette.body.clone(), palette.faint.clone());
    let first = index == 0;
    let title_size = if first { 2.6 } else { 1.5 };
    let title_y = if first { 3.6 } else { 5.6 };
    builder.text(&Spec::new(slide.kicker, Face::Bold, 0.55), &accent, Vec2::new(LEFT, title_y + title_size * 1.15), 0.0);
    builder.text(&Spec::new(slide.title, Face::Serif, title_size).fit(33.0).depth(title_size * 0.16), &title, Vec2::new(LEFT, title_y), 0.05);
    builder.block(Cuboid::new(4.0, 0.08, 0.08).into(), &accent, Vec3::new(LEFT + 2.0, title_y - 0.8, 0.0), 0.15);
    let line_size = if first { 0.9 } else { 0.62 };
    let mut y = title_y - 2.2;
    for (step, line) in slide.lines.iter().enumerate() {
        let delay = 0.25 + f32::from(u16::try_from(step).unwrap_or(0)) * 0.08;
        builder.text(&Spec::new(line, Face::Sans, line_size).fit(16.5), &body, Vec2::new(LEFT, y), delay);
        y -= line_size * 1.6;
    }
    let page = format!("{} / {}", index + 1, SLIDES.len());
    builder.text(&Spec::new(&page, Face::Sans, 0.4).align(Align::Right), &faint, Vec2::new(-LEFT, -8.8), 0.3);
    builder.text(
        &Spec::new("\u{2192} next    \u{2190} back    n notes    esc arena", Face::Sans, 0.4),
        &faint,
        Vec2::new(LEFT, -8.8),
        0.3,
    );
}

fn showcase(builder: &mut Builder<'_, '_, '_>, columns: u32, rows: u32, moves: &[u32]) {
    let size = BoardSize { columns, rows, label: "" };
    let scale = (11.0 / (board_top(size) + 0.5)).min(15.0 / (f32_of(columns) + 1.0));
    builder.board(size, Vec3::new(8.5, -6.8, 0.0), scale, moves, 0.2, true);
}

fn boards(builder: &mut Builder<'_, '_, '_>, list: &[(u32, u32, &str)]) {
    let scale = 0.52;
    let widths: Vec<f32> = list.iter().map(|&(columns, ..)| (f32_of(columns) + 1.0) * scale).collect();
    let gap = 0.8;
    let total: f32 = widths.iter().sum::<f32>() + gap * f32_of(u32::try_from(list.len()).unwrap_or(1).saturating_sub(1));
    let mut x = 8.6 - total / 2.0;
    let caption = builder.palette.body.clone();
    for (step, (&(columns, rows, label), width)) in list.iter().zip(&widths).enumerate() {
        let centre = x + width / 2.0;
        let delay = 0.3 + f32_of(u32::try_from(step).unwrap_or(0)) * 0.25;
        builder.board(BoardSize { columns, rows, label: "" }, Vec3::new(centre, -4.8, 0.0), scale, &[], delay, false);
        builder.text(&Spec::new(label, Face::Bold, 0.5).align(Align::Center), &caption, Vec2::new(centre, -6.1), delay + 0.2);
        x += width + gap;
    }
}

fn bar_chart(builder: &mut Builder<'_, '_, '_>, bars: &Bars) {
    let palette = builder.palette;
    let (faint, body, title, accent, plain) =
        (palette.faint.clone(), palette.body.clone(), palette.title.clone(), palette.accent.clone(), palette.bar.clone());
    let label_right = 5.4;
    let start = 5.8;
    let full = 9.6;
    let count = f32_of(u32::try_from(bars.items.len()).unwrap_or(1).max(1));
    let spacing = (10.0 / count).min(1.7);
    let thickness = spacing * 0.62;
    builder.text(&Spec::new(bars.caption, Face::Sans, 0.45).fit(16.5), &faint, Vec2::new(5.8, 4.4), 0.2);
    let mut y = 3.0 - thickness / 2.0;
    for (step, &(label, value, shown, highlight)) in bars.items.iter().enumerate() {
        let delay = 0.35 + f32_of(u32::try_from(step).unwrap_or(0)) * 0.12;
        let share = ((value - bars.floor) / (bars.ceiling - bars.floor)).clamp(0.015, 1.0);
        let length = share * full;
        let text_y = y - 0.2;
        builder.text(
            &Spec::new(label, Face::Sans, 0.46).align(Align::Right).fit(label_right - 0.6),
            &body,
            Vec2::new(label_right, text_y),
            delay,
        );
        let fill = if highlight { accent.clone() } else { plain.clone() };
        let mesh = builder.meshes.add(Cuboid::new(1.0, thickness, 0.6));
        builder.commands.spawn((
            SlideItem,
            Mesh3d(mesh),
            MeshMaterial3d(fill),
            Transform::from_translation(STAGE + Vec3::new(start, y, 0.0)).with_scale(Vec3::new(0.001, 1.0, 1.0)),
            Grow {
                start,
                length,
                delay,
                age: 0.0,
            },
        ));
        let value_material = if highlight { accent.clone() } else { title.clone() };
        builder.text(&Spec::new(shown, Face::Bold, 0.46), &value_material, Vec2::new(start + length + 0.3, text_y), delay + 0.45);
        y -= spacing;
    }
}

fn family_index(family: &str) -> usize {
    match family {
        "Search" => 0,
        "Search + learning" => 1,
        "Learning" => 2,
        "Knowledge" => 3,
        _ => 4,
    }
}

fn agents(builder: &mut Builder<'_, '_, '_>) {
    let palette = builder.palette;
    let (tile, title, faint) = (palette.tile.clone(), palette.title.clone(), palette.faint.clone());
    let families = palette.families.clone();
    for (index, agent) in ROSTER.iter().enumerate() {
        let column = f32_of(u32::try_from(index % 2).unwrap_or(0));
        let row = f32_of(u32::try_from(index / 2).unwrap_or(0));
        let centre = Vec2::new(4.6 + column * 8.0, 3.0 - row * 2.45);
        let delay = 0.3 + f32_of(u32::try_from(index).unwrap_or(0)) * 0.08;
        builder.block(Cuboid::new(7.6, 2.0, 0.25).into(), &tile, centre.extend(-0.2), delay);
        let icon = Cylinder::new(0.42, 0.25);
        let entity = builder.block(icon.into(), &families[family_index(agent.family)], (centre + Vec2::new(-3.0, 0.0)).extend(0.05), delay);
        builder.commands.entity(entity).insert(Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)));
        builder.text(&Spec::new(agent.title, Face::Bold, 0.5).fit(5.6), &title, centre + Vec2::new(-2.3, 0.05), delay + 0.1);
        let note = format!("{}  \u{b7}  Elo {}", agent.family.to_lowercase(), agent.elo.round());
        builder.text(&Spec::new(&note, Face::Sans, 0.36).fit(5.6), &faint, centre + Vec2::new(-2.3, -0.6), delay + 0.15);
    }
}

fn network(builder: &mut Builder<'_, '_, '_>) {
    let palette = builder.palette;
    let (layer, accent, plain, body) = (palette.layer.clone(), palette.accent.clone(), palette.bar.clone(), palette.body.clone());
    let size = BoardSize {
        columns: 8,
        rows: 7,
        label: "",
    };
    builder.board(size, Vec3::new(1.8, -2.6, 0.0), 0.42, &[3, 4, 3, 3, 4, 2, 5, 5, 2], 0.2, false);
    for step in 0..6_u16 {
        let x = 5.4 + f32::from(step) * 1.2;
        let delay = 0.5 + f32::from(step) * 0.12;
        let entity = builder.block(Cuboid::new(0.3, 4.6, 4.6).into(), &layer, Vec3::new(x, 0.0, 0.0), delay);
        builder
            .commands
            .entity(entity)
            .insert((Transform::from_rotation(Quat::from_rotation_y(-0.55)), Pulse(f32::from(step) * 0.6)));
    }
    let scores = [0.35_f32, 0.5, 0.62, 0.95, 0.7, 0.4, 0.3, 0.2];
    for (col, score) in scores.iter().enumerate() {
        let x = 13.1 + f32_of(u32::try_from(col).unwrap_or(0)) * 0.45;
        let height = score * 3.6;
        let fill = if col == 3 { accent.clone() } else { plain.clone() };
        let delay = 1.3 + f32_of(u32::try_from(col).unwrap_or(0)) * 0.05;
        builder.block(Cuboid::new(0.32, height, 0.32).into(), &fill, Vec3::new(x, -2.3 + height / 2.0, 0.0), delay);
    }
    let captions = [(1.8, "a board in"), (8.4, "6 layers x 64"), (14.7, "a score per column")];
    for (x, caption) in captions {
        builder.text(&Spec::new(caption, Face::Sans, 0.42).align(Align::Center), &body, Vec2::new(x, -4.4), 1.0);
    }
}

fn solved_board(builder: &mut Builder<'_, '_, '_>, columns: Option<Vec<Option<Outcome>>>) {
    let size = BoardSize {
        columns: 7,
        rows: 6,
        label: "",
    };
    let moves = solved_columns();
    let scale = 1.05;
    let base = Vec3::new(8.8, -6.6, 0.0);
    builder.board(size, base, scale, &moves, 0.2, false);
    let label_y = base.y + (board_top(size) + 0.55) * scale;
    let mover = if moves.len() % 2 == 0 { "red" } else { "yellow" };
    let palette = builder.palette;
    let outcomes = palette.outcomes.clone();
    let (faint, body) = (palette.faint.clone(), palette.body.clone());
    match columns {
        Some(columns) => {
            for (col, outcome) in columns.iter().enumerate() {
                let Some(outcome) = outcome else { continue };
                let (letter, index) = match outcome {
                    Outcome::Win => ("W", 2),
                    Outcome::Draw => ("D", 1),
                    Outcome::Loss => ("L", 0),
                };
                let x = base.x + cell_x(size, u32::try_from(col).unwrap_or(0)) * scale;
                let delay = 1.9 + f32_of(u32::try_from(col).unwrap_or(0)) * 0.1;
                builder.text(&Spec::new(letter, Face::Bold, 0.7).align(Align::Center), &outcomes[index], Vec2::new(x, label_y), delay);
            }
            let caption = format!("if {mover} plays here: win, draw or loss");
            builder.text(&Spec::new(&caption, Face::Sans, 0.42).align(Align::Center), &body, Vec2::new(base.x, label_y + 1.0), 2.0);
        }
        None => {
            builder.text(&Spec::new("solving...", Face::Sans, 0.5).align(Align::Center), &faint, Vec2::new(base.x, label_y), 0.5);
        }
    }
}

fn f32_of(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// Holds the camera on the stage, framing the 16:9 layout whatever the window shape.
/// Only the main scene camera (not the HUD's) flies to the stage.
#[allow(clippy::type_complexity)]
fn slide_camera(
    time: Res<Time>,
    deck: Res<Deck>,
    mut kick: ResMut<Kick>,
    windows: Query<&Window>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut lights: Query<&mut PointLight, With<StageLight>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let aspect = window.width().max(1.0) / window.height().max(1.0);
    let half_fov = (FRAC_PI_4 / 2.0).tan();
    let distance = VIEW_DISTANCE.max(HALF_WIDTH / (half_fov * aspect)).max(HALF_HEIGHT / half_fov);
    let t = time.elapsed_secs();
    let drift = Vec3::new((t * 0.21).sin() * 0.6, (t * 0.17).sin() * 0.3, 0.0);
    // Each slide is seen from a slightly different angle, so changing slide moves the camera.
    let step = f32_of(u32::try_from(deck.index).unwrap_or(0));
    // Small enough that nothing leaves the frame; the look point moves with it, so the
    // view shifts rather than turning the slide out of shot.
    let angle = Vec3::new((step * 1.7).sin() * 1.0, (step * 1.3).cos() * 0.45, 0.0);
    // A quick push-in and a flash of the key light each time the slide changes.
    kick.0 += time.delta_secs();
    let punch = (-kick.0 * 6.0).exp();
    for mut light in &mut lights {
        light.intensity = STAGE_LIGHT * (1.0 + 1.4 * punch);
    }
    let wanted = STAGE + Vec3::new(0.0, 0.0, distance - 2.2 * punch) + drift + angle;
    let blend = 1.0 - (-time.delta_secs() * 3.0).exp();
    for mut transform in &mut cameras {
        transform.translation = transform.translation.lerp(wanted, blend);
        let look = STAGE + drift * 0.5 + angle * 0.85;
        let target = transform.looking_at(look, Vec3::Y).rotation;
        transform.rotation = transform.rotation.slerp(target, blend);
    }
}

/// easeOutBack: overshoots a little, then settles at 1.
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u.powi(3) + c1 * u.powi(2)
}

const APPEAR_SECONDS: f32 = 0.55;

fn appear(mut commands: Commands, time: Res<Time>, mut items: Query<(Entity, &mut Appear, &mut Transform, Option<&Scale>)>) {
    let dt = time.delta_secs();
    for (entity, mut item, mut transform, scale) in &mut items {
        item.age += dt;
        let t = ((item.age - item.delay) / APPEAR_SECONDS).clamp(0.0, 1.0);
        let full = scale.map_or(1.0, |scale| scale.0);
        let eased = if t <= 0.0 { 0.0001 } else { ease_out_back(t).max(0.0001) };
        transform.scale = Vec3::splat(full * eased);
        let rest = (1.0 - t).powi(2);
        transform.translation = item.home + Vec3::new(item.side * rest * 7.0, rest * -0.8, rest * -3.0);
        if t >= 1.0 {
            commands.entity(entity).remove::<Appear>();
        }
    }
}

const LEAVE_SECONDS: f32 = 0.45;

/// Old pieces accelerate away sideways and back, shrinking, then are removed.
fn leave_slide(mut commands: Commands, time: Res<Time>, mut items: Query<(Entity, &mut Leaving, &mut Transform)>) {
    for (entity, mut item, mut transform) in &mut items {
        item.age += time.delta_secs();
        let t = (item.age / LEAVE_SECONDS).clamp(0.0, 1.0);
        let ease = t * t;
        transform.translation = item.from + Vec3::new(-item.side * 16.0 * ease, 1.5 * ease, -8.0 * ease);
        transform.scale = item.scale * (1.0 - ease).max(0.0001);
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn grow(time: Res<Time>, mut bars: Query<(&mut Grow, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut bar, mut transform) in &mut bars {
        bar.age += dt;
        let t = ((bar.age - bar.delay) / 0.9).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let length = (bar.length * eased).max(0.001);
        transform.scale.x = length;
        transform.translation.x = STAGE.x + bar.start + length / 2.0;
    }
}

fn drop_discs(
    mut commands: Commands,
    time: Res<Time>,
    mut discs: Query<(Entity, &mut Drop, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (entity, mut disc, mut transform, mut visibility) in &mut discs {
        if disc.delay > 0.0 {
            disc.delay -= dt;
            continue;
        }
        *visibility = Visibility::Inherited;
        disc.velocity -= 40.0 * dt;
        transform.translation.y += disc.velocity * dt;
        if transform.translation.y <= disc.rest {
            transform.translation.y = disc.rest;
            if disc.velocity.abs() * 0.25 > 1.5 {
                disc.velocity = -disc.velocity * 0.25;
            } else {
                commands.entity(entity).remove::<Drop>();
            }
        }
    }
}

fn sway(time: Res<Time>, mut items: Query<(&Sway, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (sway, mut transform) in &mut items {
        transform.rotation = Quat::from_rotation_y((t * sway.speed + sway.phase).sin() * sway.amount);
    }
}

fn pulse(time: Res<Time>, mut layers: Query<(&Pulse, &mut Transform), Without<Appear>>) {
    let t = time.elapsed_secs();
    for (pulse, mut transform) in &mut layers {
        let wave = ((t * 2.2 - pulse.0).sin() * 0.5 + 0.5).powi(4);
        transform.scale = Vec3::new(1.0 + wave * 0.6, 1.0, 1.0);
    }
}
