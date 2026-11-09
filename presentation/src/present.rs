//! Presentation mode: a slide deck of 3D visuals with a short 2D title on each.
//!
//! The slides live on a "stage" far from the arena table. Entering the mode hides the
//! arena UI and flies the camera to the stage; Esc flies back. Right / Space / click go
//! forward, Left goes back. Each slide is a few beats: Next reveals the slide's next
//! piece (a bar, a group of agents, the board growing) before moving to the next slide.
//! Content is in `slides.rs`; what to say is in the speaker notes, not on the slides.
//!
//! All words are flat 2D text, pinned to points on the stage: they follow the camera's
//! drift but always face the screen at a readable size.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use bevy::picking::Pickable;
use bevy::prelude::*;
use c4::outcome::Outcome;

use crate::roster::ROSTER;
use crate::scene::{SceneAssets, Shape, board_parts, board_top, cell_x, cell_y, morph_shaped_board, spawn_shaped_board};
use crate::sizes::BoardSize;
use crate::slides::{Bars, SLIDES, Slide, Stage, Visual, solve_position, solved_columns};
use crate::theme::{ACCENT, AQUA, BOARD, FAINT, GREEN, MUTED, RAISED, RED, TEXT, YELLOW, bold_font, title_font, ui_font};

/// Where the slides are built, well away from the arena table.
pub const STAGE: Vec3 = Vec3::new(0.0, 0.0, -140.0);
/// Camera distance from the stage plane; the layout below is sized for 16:9 at this distance.
const VIEW_DISTANCE: f32 = 24.0;
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
    /// Which beat of the slide is showing: each press of Next reveals one more.
    pub beat: usize,
    /// The last beat whose pieces are on stage.
    built_beat: usize,
    /// The whole slide needs building (new slide, or stepping back).
    dirty: bool,
    /// Only the beats after `built_beat` need adding.
    stepped: bool,
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
        self.beat = 0;
        self.dirty = true;
    }

    pub fn beats(&self) -> usize {
        SLIDES[self.index].beats.len().max(1)
    }

    /// One step on: the slide's next beat, else the next slide. False at the very end.
    pub fn next(&mut self) -> bool {
        if self.beat + 1 < self.beats() {
            self.beat += 1;
            self.direction = 1.0;
            self.stepped = true;
            true
        } else if self.index + 1 < SLIDES.len() {
            let index = self.index + 1;
            self.go_to(index);
            true
        } else {
            false
        }
    }

    /// One step back: the previous beat (the slide is rebuilt as it was then), else the
    /// previous slide at its last beat.
    pub fn back(&mut self) {
        if self.beat > 0 {
            self.beat -= 1;
            self.direction = -1.0;
            self.dirty = true;
        } else if self.index > 0 {
            let index = self.index - 1;
            self.go_to(index);
            self.beat = self.beats() - 1;
        }
    }
}

#[derive(Resource)]
struct Palette {
    accent: Handle<StandardMaterial>,
    bar: Handle<StandardMaterial>,
    tile: Handle<StandardMaterial>,
    layer: Handle<StandardMaterial>,
    families: [Handle<StandardMaterial>; 5],
}

const OUTCOMES: [Color; 3] = [RED, YELLOW, GREEN];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Serif,
    Sans,
    Bold,
}

impl Face {
    fn source(self) -> bevy::text::FontSource {
        match self {
            Self::Serif => title_font(),
            Self::Sans => ui_font(),
            Self::Bold => bold_font(),
        }
    }

    /// Rough average glyph width in ems, for fitting a line into a width.
    const fn width(self) -> f32 {
        match self {
            Self::Serif => 0.47,
            Self::Sans => 0.5,
            Self::Bold => 0.53,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// One line of slide text: what, in which face, how big (world units on the stage,
/// one em), how it lines up on its point, and an optional width to shrink it into.
struct Spec<'a> {
    text: &'a str,
    face: Face,
    size: f32,
    align: Align,
    fit: Option<f32>,
}

impl<'a> Spec<'a> {
    const fn new(text: &'a str, face: Face, size: f32) -> Self {
        Self {
            text,
            face,
            size,
            align: Align::Left,
            fit: None,
        }
    }

    const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    const fn fit(mut self, width: f32) -> Self {
        self.fit = Some(width);
        self
    }

    /// The size, shrunk if the line would be wider than `fit`.
    fn fitted_size(&self) -> f32 {
        let chars = f32_of(u32::try_from(self.text.chars().count()).unwrap_or(1).max(1));
        let wide = chars * self.size * self.face.width();
        match self.fit {
            Some(width) if wide > width => self.size * width / wide,
            _ => self.size,
        }
    }
}

/// A line of 2D slide text pinned to a point on the stage. The outer node sits on the
/// projected point; the text inside is shifted so its baseline and edge line up there.
#[derive(Component)]
struct SlideText {
    at: Vec3,
    size: f32,
    colour: Color,
    delay: f32,
    age: f32,
    side: f32,
    leaving: Option<f32>,
    label: Entity,
    /// The last beat this line belongs to; it leaves when the slide steps past it.
    until: Option<usize>,
}

/// A piece that leaves when the slide steps past this beat.
#[derive(Component)]
struct Until(usize);

/// The growing board: which stage it shows and what it has in it.
#[derive(Component)]
struct Growing {
    stages: &'static [Stage],
    shown: usize,
    /// The board has cell tiles for this size.
    cells: BoardSize,
    heights: [u32; 16],
    played: usize,
}

/// A disc on the growing board: it follows its column as the board widens.
#[derive(Component)]
struct StageDisc {
    col: u32,
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
            .init_resource::<Deck>()
            .init_resource::<Kick>()
            .add_systems(Startup, (setup, start_solver))
            .add_systems(OnEnter(Mode::Slides), enter)
            .add_systems(OnExit(Mode::Slides), leave)
            .add_systems(
                Update,
                (
                    navigate,
                    poll_solver,
                    build_slide,
                    grow_stage,
                    slide_camera,
                    appear,
                    leave_slide,
                    grow,
                    drop_discs,
                    place_stage_discs,
                    sway,
                    pulse,
                    place_text,
                )
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
        accent: make(ACCENT, 0.45),
        bar: make(MUTED.mix(&RAISED, 0.55), 0.1),
        tile: make(RAISED, 0.05),
        layer: make(BOARD.mix(&AQUA, 0.35), 0.15),
        families: [make(ACCENT, 0.4), make(YELLOW, 0.4), make(AQUA, 0.4), make(GREEN, 0.4), make(FAINT, 0.3)],
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

fn leave(mut commands: Commands, items: Query<Entity, Or<(With<SlideItem>, With<SlideText>)>>) {
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
        if !deck.next() {
            mode.set(Mode::Arena);
        }
    } else if back {
        deck.back();
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
    meshes: &'a mut Assets<Mesh>,
    palette: &'a Palette,
    assets: &'a SceneAssets,
    /// Which side new pieces sweep in from.
    side: f32,
    /// The beats being built this time (all of them up to `to` on a fresh slide, just
    /// the new ones when stepping).
    from: usize,
    to: usize,
    /// The beat the next pieces belong to, and the last beat they stay for.
    beat: usize,
    until: Option<usize>,
}

impl Builder<'_, '_, '_> {
    /// Whether pieces for `beat` go up now.
    fn shows(&self) -> bool {
        (self.from..=self.to).contains(&self.beat) && self.until.is_none_or(|until| until >= self.to)
    }

    /// Delay for something revealed on a step: quick, since the slide is already up.
    fn soon(&self, fresh: f32, step: f32) -> f32 {
        if self.from == 0 { fresh } else { step }
    }

    /// Spawns a line of 2D text pinned at `at` on the stage; it fades in after `delay`.
    fn text(&mut self, spec: &Spec<'_>, colour: Color, at: Vec2, delay: f32) {
        if spec.text.is_empty() || !self.shows() {
            return;
        }
        let (share, wrap) = match spec.align {
            Align::Left => (0.0, Justify::Left),
            Align::Center => (-50.0, Justify::Center),
            Align::Right => (-100.0, Justify::Right),
        };
        let label = self
            .commands
            .spawn((
                Text::new(spec.text),
                TextFont {
                    font: spec.face.source(),
                    font_size: FontSize::Px(1.0),
                    weight: if spec.face == Face::Serif { FontWeight::THIN } else { FontWeight::default() },
                    ..default()
                },
                TextColor(colour.with_alpha(0.0)),
                TextLayout::new(wrap, LineBreak::NoWrap),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    ..default()
                },
                // The baseline sits about three quarters of the way down a line.
                UiTransform::from_translation(Val2::percent(share, -76.0)),
                Pickable::IGNORE,
            ))
            .id();
        self.commands
            .spawn((
                SlideText {
                    at: STAGE + at.extend(0.0),
                    size: spec.fitted_size(),
                    colour,
                    delay,
                    age: 0.0,
                    side: self.side,
                    leaving: None,
                    label,
                    until: self.until,
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                GlobalZIndex(5),
                Pickable::IGNORE,
            ))
            .add_child(label);
    }

    fn block(&mut self, mesh: Mesh, material: &Handle<StandardMaterial>, at: Vec3, delay: f32) -> Option<Entity> {
        if !self.shows() {
            return None;
        }
        let mesh = self.meshes.add(mesh);
        let mut entity = self
            .commands
            .spawn((SlideItem, appear_at(STAGE + at, delay, self.side), Mesh3d(mesh), MeshMaterial3d(material.clone())));
        if let Some(until) = self.until {
            entity.insert(Until(until));
        }
        Some(entity.id())
    }

    /// A board standing at `base` (bottom centre) with the given scale and discs.
    fn board(&mut self, size: BoardSize, base: Vec3, scale: f32, moves: &[u32], delay: f32, sway: bool) -> Option<Entity> {
        if !self.shows() {
            return None;
        }
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
        Some(root.id())
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
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<Palette>,
    assets: Res<SceneAssets>,
    old: Query<(Entity, &Transform, Option<&Until>), (With<SlideItem>, Without<Leaving>)>,
    mut old_text: Query<&mut SlideText>,
) {
    let full = deck.dirty;
    if !full && !deck.stepped {
        return;
    }
    deck.dirty = false;
    deck.stepped = false;
    let (from, to) = if full { (0, deck.beat) } else { (deck.built_beat + 1, deck.beat) };
    deck.built_beat = deck.beat;
    if from > to {
        return;
    }
    // A new slide gets the full push-in and flash; a step gets a small nudge.
    kick.0 = if full { 0.0 } else { kick.0.max(0.3) };
    // On a new slide everything old flies out the way we are going; on a step only the
    // pieces whose beat is over do.
    for (entity, transform, until) in &old {
        if full || until.is_some_and(|until| until.0 < to) {
            commands.entity(entity).remove::<(Appear, Grow)>().insert(Leaving {
                age: 0.0,
                from: transform.translation,
                scale: transform.scale,
                side: deck.direction,
            });
        }
    }
    for mut text in &mut old_text {
        if text.leaving.is_none() && (full || text.until.is_some_and(|until| until < to)) {
            text.leaving = Some(0.0);
            text.side = deck.direction;
        }
    }
    let slide = &SLIDES[deck.index];
    let solved = deck.solved.lock().ok().and_then(|slot| slot.clone());
    deck.waiting_for_solve = matches!(slide.visual, Visual::Solved) && solved.is_none();
    let mut builder = Builder {
        commands: &mut commands,
        meshes: &mut meshes,
        palette: &palette,
        assets: &assets,
        side: deck.direction,
        from,
        to,
        beat: 0,
        until: None,
    };
    frame(&mut builder, slide, deck.index);
    builder.beat = 0;
    builder.until = None;
    match &slide.visual {
        Visual::Board { columns, rows, moves } => showcase(&mut builder, *columns, *rows, moves),
        Visual::Expand(stages) => expand(&mut builder, stages),
        Visual::Bars(bars) => bar_chart(&mut builder, bars),
        Visual::Agents(beats) => agents(&mut builder, beats),
        Visual::Network => network(&mut builder),
        Visual::Solved => solved_board(&mut builder, solved.flatten()),
    }
}

/// The words on every slide: a small kicker, the title, a rule and the page number.
/// Everything else is said aloud (see the speaker notes).
fn frame(builder: &mut Builder<'_, '_, '_>, slide: &Slide, index: usize) {
    let accent = builder.palette.accent.clone();
    let title_y = 6.4;
    builder.text(&Spec::new(slide.kicker, Face::Bold, 0.5).align(Align::Center), ACCENT, Vec2::new(0.0, 8.3), 0.0);
    builder.text(&Spec::new(slide.title, Face::Serif, 1.45).align(Align::Center).fit(32.0), TEXT, Vec2::new(0.0, title_y), 0.05);
    builder.block(Cuboid::new(3.0, 0.07, 0.07).into(), &accent, Vec3::new(0.0, title_y - 0.75, 0.0), 0.15);
    let page = format!("{} / {}", index + 1, SLIDES.len());
    builder.text(&Spec::new(&page, Face::Sans, 0.4).align(Align::Right), FAINT, Vec2::new(16.8, -9.1), 0.3);
    // One dot per beat over the page number; they fill in as the slide steps on.
    let count = slide.beats.len();
    if count > 1 {
        let (plain, accent) = (builder.palette.bar.clone(), builder.palette.accent.clone());
        for step in 0..count {
            let x = 16.55 - f32_of(u32::try_from(count - 1 - step).unwrap_or(0)) * 0.5;
            builder.beat = 0;
            builder.block(Sphere::new(0.1).into(), &plain, Vec3::new(x, -8.3, 0.0), 0.3);
            builder.beat = step;
            let delay = builder.soon(0.35, 0.0);
            builder.block(Sphere::new(0.15).into(), &accent, Vec3::new(x, -8.3, 0.1), delay);
        }
    }
    // A short line under the visual for the beats that have one; it goes with its beat.
    for (step, beat) in slide.beats.iter().enumerate() {
        builder.beat = step;
        builder.until = Some(step);
        let delay = builder.soon(0.7, 0.25);
        builder.text(&Spec::new(beat.caption, Face::Bold, 0.62).align(Align::Center).fit(30.0), ACCENT, Vec2::new(0.0, -8.9), delay);
    }
}

fn showcase(builder: &mut Builder<'_, '_, '_>, columns: u32, rows: u32, moves: &[u32]) {
    let size = BoardSize { columns, rows, label: "" };
    let scale = (12.6 / (board_top(size) + 0.6)).min(28.0 / (f32_of(columns) + 1.0));
    builder.board(size, Vec3::new(0.0, -8.6, 0.0), scale, moves, 0.2, true);
}

/// One board that grows a column and a row at a time, with its size written beside it.
fn expand(builder: &mut Builder<'_, '_, '_>, stages: &'static [Stage]) {
    let Some(last) = stages.len().checked_sub(1) else {
        return;
    };
    let shown = builder.to.min(last);
    let base = Vec3::new(-3.8, -8.1, 0.0);
    let scale = 1.2;
    let size_of = |stage: &Stage| BoardSize {
        columns: stage.columns,
        rows: stage.rows,
        label: "",
    };
    if builder.from == 0 {
        let size = size_of(&stages[shown]);
        let root = builder
            .commands
            .spawn((
                SlideItem,
                appear_at(STAGE + base, 0.2, builder.side),
                Transform::from_translation(STAGE + base).with_scale(Vec3::splat(scale)),
                Visibility::default(),
                Scale(scale),
                Sway {
                    amount: 0.16,
                    speed: 0.3,
                    phase: 0.0,
                },
            ))
            .id();
        spawn_shaped_board(builder.commands, builder.assets, root, size);
        let mut growing = Growing {
            stages,
            shown,
            cells: size,
            heights: [0; 16],
            played: 0,
        };
        let mut delay = 0.55;
        for stage in &stages[..=shown] {
            for &col in stage.moves {
                drop_stage_disc(builder.commands, builder.assets, root, &mut growing, size, col, delay);
                delay += 0.12;
            }
        }
        builder.commands.entity(root).insert(growing);
    }
    // The size, big, to the right of the board; it changes as the board grows.
    let x = 4.4;
    for (step, stage) in stages.iter().enumerate() {
        builder.beat = step;
        builder.until = (step < last).then_some(step);
        let delay = builder.soon(0.45, 1.0);
        let ink = if step == last || stage.columns == 9 { ACCENT } else { TEXT };
        builder.text(&Spec::new(stage.name, Face::Serif, 2.6), ink, Vec2::new(x, 0.9), delay);
        for (line, text) in stage.note.lines().enumerate() {
            let y = -0.6 - f32_of(u32::try_from(line).unwrap_or(0)) * 0.95;
            builder.text(&Spec::new(text, Face::Sans, 0.68).fit(12.5), MUTED, Vec2::new(x, y), delay + 0.15);
        }
    }
    builder.until = None;
}

/// Drops a disc into `col` of a growing board (sized `size` once it is done growing).
fn drop_stage_disc(
    commands: &mut Commands,
    assets: &SceneAssets,
    root: Entity,
    growing: &mut Growing,
    size: BoardSize,
    col: u32,
    delay: f32,
) {
    let Some(height) = growing.heights.get_mut(usize::try_from(col).unwrap_or(usize::MAX)) else {
        return;
    };
    if *height >= size.rows {
        return;
    }
    let row = *height;
    *height += 1;
    let player = growing.played % 2;
    growing.played += 1;
    commands.spawn((
        StageDisc { col },
        Mesh3d(assets.disc.clone()),
        MeshMaterial3d(assets.solid[player].clone()),
        Transform::from_xyz(0.0, board_top(size) + 1.5, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        Drop {
            rest: cell_y(row),
            delay,
            velocity: 0.0,
        },
        Visibility::Hidden,
        ChildOf(root),
    ));
}

/// When the deck steps on, the growing board morphs to the next stage (the arena's
/// resize: the frame slides out as new columns unfold, then the top rises as new rows
/// grow in), then that stage's discs drop.
fn grow_stage(
    mut commands: Commands,
    deck: Res<Deck>,
    assets: Res<SceneAssets>,
    mut roots: Query<(Entity, &mut Growing, &Shape), Without<Leaving>>,
) {
    for (root, mut growing, shape) in &mut roots {
        if deck.beat <= growing.shown {
            continue;
        }
        let last = growing.stages.len().saturating_sub(1);
        let target = deck.beat.min(last);
        let mut delay = 0.0;
        for step in growing.shown + 1..=target {
            let stage = &growing.stages[step];
            let size = BoardSize {
                columns: stage.columns,
                rows: stage.rows,
                label: "",
            };
            if size.columns != growing.cells.columns || size.rows != growing.cells.rows {
                morph_shaped_board(&mut commands, &assets, root, (shape.w, shape.h), growing.cells, size);
                growing.cells = size;
                delay = 1.4;
            }
            for &col in stage.moves {
                drop_stage_disc(&mut commands, &assets, root, &mut growing, size, col, delay);
                delay += 0.3;
            }
        }
        growing.shown = deck.beat;
    }
}

/// Keeps the growing board's discs centred on their columns while it widens.
fn place_stage_discs(roots: Query<&Shape, With<Growing>>, mut discs: Query<(&StageDisc, &ChildOf, &mut Transform)>) {
    for (disc, parent, mut transform) in &mut discs {
        if let Ok(shape) = roots.get(parent.parent()) {
            transform.translation.x = f32_of(disc.col) + 0.5 - shape.w / 2.0;
        }
    }
}

fn bar_chart(builder: &mut Builder<'_, '_, '_>, bars: &Bars) {
    let (accent, plain) = (builder.palette.accent.clone(), builder.palette.bar.clone());
    let label_right = -5.0;
    let start = -4.4;
    let full = 15.5;
    let count = f32_of(u32::try_from(bars.items.len()).unwrap_or(1).max(1));
    let spacing = (11.4 / count).min(2.3);
    let thickness = spacing * 0.62;
    let label_size = (spacing * 0.36).clamp(0.45, 0.62);
    builder.text(&Spec::new(bars.caption, Face::Sans, 0.48).align(Align::Center), FAINT, Vec2::new(0.0, 4.75), 0.2);
    let mut y = 4.0 - thickness / 2.0;
    let mut in_beat = [0_u32; 16];
    for &(label, value, shown, highlight, beat) in bars.items {
        builder.beat = beat;
        let rank = in_beat.get_mut(beat).map_or(0, |count| {
            *count += 1;
            *count - 1
        });
        let delay = builder.soon(0.35, 0.1) + f32_of(rank) * 0.15;
        if !builder.shows() {
            y -= spacing;
            continue;
        }
        let share = ((value - bars.floor) / (bars.ceiling - bars.floor)).clamp(0.015, 1.0);
        let length = share * full;
        let text_y = y - label_size * 0.36;
        builder.text(&Spec::new(label, Face::Sans, label_size).align(Align::Right).fit(10.5), MUTED, Vec2::new(label_right, text_y), delay);
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
        let ink = if highlight { ACCENT } else { TEXT };
        builder.text(&Spec::new(shown, Face::Bold, label_size), ink, Vec2::new(start + length + 0.35, text_y), delay + 0.45);
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

fn agents(builder: &mut Builder<'_, '_, '_>, beats: &[usize]) {
    let tile = builder.palette.tile.clone();
    let families = builder.palette.families.clone();
    let mut in_beat = [0_u32; 16];
    for (index, agent) in ROSTER.iter().enumerate() {
        builder.beat = beats.get(index).copied().unwrap_or(0);
        let rank = in_beat.get_mut(builder.beat).map_or(0, |count| {
            *count += 1;
            *count - 1
        });
        let column = f32_of(u32::try_from(index % 4).unwrap_or(0));
        let row = f32_of(u32::try_from(index / 4).unwrap_or(0));
        let centre = Vec2::new(-11.7 + column * 7.8, 1.3 - row * 6.5);
        let delay = builder.soon(0.3, 0.05) + f32_of(rank) * 0.12;
        builder.block(Cuboid::new(7.3, 5.6, 0.25).into(), &tile, centre.extend(-0.2), delay);
        let icon = Cylinder::new(0.75, 0.3);
        let entity = builder.block(icon.into(), &families[family_index(agent.family)], (centre + Vec2::new(0.0, 1.3)).extend(0.05), delay);
        if let Some(entity) = entity {
            builder.commands.entity(entity).insert(Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)));
        }
        builder.text(&Spec::new(agent.title, Face::Bold, 0.56).align(Align::Center).fit(6.8), TEXT, centre + Vec2::new(0.0, -0.7), delay + 0.1);
        let note = format!("{}  \u{b7}  Elo {}", agent.family.to_lowercase(), agent.elo.round());
        builder.text(&Spec::new(&note, Face::Sans, 0.44).align(Align::Center).fit(6.8), FAINT, centre + Vec2::new(0.0, -1.8), delay + 0.15);
    }
}

fn network(builder: &mut Builder<'_, '_, '_>) {
    let (layer, accent, plain) = (builder.palette.layer.clone(), builder.palette.accent.clone(), builder.palette.bar.clone());
    let size = BoardSize {
        columns: 8,
        rows: 7,
        label: "",
    };
    let captions = [(-11.5, "a board goes in"), (0.0, "6 layers \u{d7} 64 channels"), (10.55, "a score for each column")];
    builder.beat = 0;
    builder.board(size, Vec3::new(-11.5, -4.6, 0.0), 0.9, &[3, 4, 3, 3, 4, 2, 5, 5, 2], 0.2, false);
    builder.beat = 1;
    let start = builder.soon(0.5, 0.1);
    for step in 0..6_u16 {
        let x = -4.3 + f32::from(step) * 1.7;
        let delay = start + f32::from(step) * 0.12;
        if let Some(entity) = builder.block(Cuboid::new(0.4, 6.4, 6.4).into(), &layer, Vec3::new(x, -0.6, 0.0), delay) {
            builder
                .commands
                .entity(entity)
                .insert((Transform::from_rotation(Quat::from_rotation_y(-0.55)), Pulse(f32::from(step) * 0.6)));
        }
    }
    builder.beat = 2;
    let start = builder.soon(1.3, 0.1);
    let scores = [0.35_f32, 0.5, 0.62, 0.95, 0.7, 0.4, 0.3, 0.2];
    for (col, score) in scores.iter().enumerate() {
        let x = 7.4 + f32_of(u32::try_from(col).unwrap_or(0)) * 0.9;
        let height = score * 6.5;
        let fill = if col == 3 { accent.clone() } else { plain.clone() };
        let delay = start + f32_of(u32::try_from(col).unwrap_or(0)) * 0.06;
        builder.block(Cuboid::new(0.6, height, 0.6).into(), &fill, Vec3::new(x, -4.6 + height / 2.0, 0.0), delay);
    }
    for (step, (x, caption)) in captions.into_iter().enumerate() {
        builder.beat = step;
        let delay = builder.soon(1.0, 0.5);
        builder.text(&Spec::new(caption, Face::Bold, 0.58).align(Align::Center), MUTED, Vec2::new(x, -6.6), delay);
    }
}

fn solved_board(builder: &mut Builder<'_, '_, '_>, columns: Option<Vec<Option<Outcome>>>) {
    let size = BoardSize {
        columns: 7,
        rows: 6,
        label: "",
    };
    let moves = solved_columns();
    let scale = 1.35;
    let base = Vec3::new(0.0, -7.9, 0.0);
    builder.beat = 0;
    builder.board(size, base, scale, &moves, 0.2, false);
    builder.beat = 1;
    let start = builder.soon(1.9, 0.1);
    let label_y = base.y + (board_top(size) + 0.5) * scale;
    let mover = if moves.len() % 2 == 0 { "red" } else { "yellow" };
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
                let delay = start + f32_of(u32::try_from(col).unwrap_or(0)) * 0.1;
                builder.text(&Spec::new(letter, Face::Bold, 0.8).align(Align::Center), OUTCOMES[index], Vec2::new(x, label_y), delay);
            }
            let caption = format!("if {mover} plays in this column: win, draw or loss");
            builder.text(&Spec::new(&caption, Face::Sans, 0.5).align(Align::Center), MUTED, Vec2::new(base.x, label_y + 1.2), start + 0.1);
        }
        None => {
            builder.text(&Spec::new("solving...", Face::Sans, 0.55).align(Align::Center), FAINT, Vec2::new(base.x, label_y), 0.5);
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
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut lights: Query<&mut PointLight, With<StageLight>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let distance = view_distance(window);
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

/// How far the camera stands from the stage for this window shape.
fn view_distance(window: &Window) -> f32 {
    let aspect = window.width().max(1.0) / window.height().max(1.0);
    let half_fov = (FRAC_PI_4 / 2.0).tan();
    VIEW_DISTANCE.max(HALF_WIDTH / (half_fov * aspect)).max(HALF_HEIGHT / half_fov)
}

/// Pins each line of slide text to its stage point, sizes it to match the stage, and
/// fades it in (sliding from the side the deck moves toward) or out.
#[allow(clippy::type_complexity)]
fn place_text(
    mut commands: Commands,
    time: Res<Time>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut items: Query<(Entity, &mut SlideText, &mut Node, &mut UiTransform)>,
    mut labels: Query<(&mut TextFont, &mut TextColor), Without<SlideText>>,
) {
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let half_fov = (FRAC_PI_4 / 2.0).tan();
    // Pixels per stage unit at the camera's resting distance: steady, so text is not
    // re-laid out every frame as the camera drifts.
    // UI units are window pixels divided by the UI scale (see `ui::fit_ui`).
    let ui = ui_scale.0.max(0.01);
    let per_unit = window.height() / (2.0 * view_distance(window) * half_fov) / ui;
    let dt = time.delta_secs();
    for (entity, mut item, mut node, mut transform) in &mut items {
        item.age += dt;
        let Ok(point) = camera.world_to_viewport(camera_transform, item.at) else {
            continue;
        };
        node.left = px(point.x / ui);
        node.top = px(point.y / ui);
        let (alpha, offset, scale) = if let Some(age) = item.leaving.as_mut() {
            *age += dt;
            let t = (*age / 0.35).min(1.0);
            if t >= 1.0 {
                commands.entity(entity).despawn();
                continue;
            }
            let ease = t * t;
            (1.0 - ease, -item.side * 140.0 * ease, 1.0 - 0.1 * ease)
        } else {
            let t = ((item.age - item.delay) / 0.5).clamp(0.0, 1.0);
            let rest = (1.0 - t).powi(3);
            (t, item.side * 90.0 * rest, 0.92 + 0.08 * ease_out_back(t))
        };
        transform.translation = Val2::px(offset, 0.0);
        transform.scale = Vec2::splat(scale);
        if let Ok((mut font, mut ink)) = labels.get_mut(item.label) {
            let wanted = FontSize::Px((item.size * per_unit).round().max(12.0));
            if font.font_size != wanted {
                font.font_size = wanted;
            }
            ink.0 = item.colour.with_alpha(alpha);
        }
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
