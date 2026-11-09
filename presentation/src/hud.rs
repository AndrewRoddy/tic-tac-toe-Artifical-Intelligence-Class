//! The arena's 3D HUD: every button, label and number is a mesh, drawn by a second
//! camera on its own render layer so the scene never covers it.
//!
//! Layout is in "virtual pixels": 900 tall, as wide as the window's aspect allows,
//! origin at the centre, y up. It mirrors the flat UI (`ui.rs`), which stays as the
//! backup: U (or the "2D UI" button) switches between them, and the flat mode also
//! turns off bloom and shadows for slow machines.
//!
//! Buttons are blocks you can feel: they lift toward you on hover, sink when pressed,
//! spring back with a flash and a ring of light when clicked.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::picking::mesh_picking::{MeshPickingCamera, MeshPickingPlugin, MeshPickingSettings};
use bevy::picking::prelude::{Pickable, PointerButton, PointerClick, PointerOut, PointerOver, PointerPress, PointerRelease};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::game::{Action, Pending, Player, Settings, UiStyle};
use crate::present::Mode;
use crate::readout::{Readout, ReadoutSet};
use crate::roster::ROSTER;
use crate::text3d::{Align, Face, Fonts, Spec, text_bundle};
use crate::theme::{ACCENT, BORDER, FAINT, HOVER, MUTED, ON_ACCENT, PANEL, RAISED, RED, TEXT, YELLOW};

/// The HUD lives on this render layer; the scene is on layer 0.
const LAYER: usize = 1;
/// Distance from the HUD camera to the HUD plane.
const DEPTH: f32 = 10.0;
/// Virtual pixels from the top to the bottom of the window.
const TALL: f32 = 900.0;
const SIDE_WIDTH: f32 = 290.0;

fn unit() -> f32 {
    2.0 * DEPTH * (FRAC_PI_4 / 2.0).tan() / TALL
}

const fn layer() -> RenderLayers {
    RenderLayers::layer(LAYER)
}

#[derive(Component)]
struct HudCamera;

#[derive(Component)]
struct HudRoot;

/// Which window edge a HUD element is measured from.
#[derive(Clone, Copy)]
enum Edge {
    Left,
    Centre,
    Right,
}

/// A HUD element's place: `x` from its edge, `y` from the centre (virtual pixels).
#[derive(Component, Clone, Copy)]
struct Anchor {
    edge: Edge,
    x: f32,
    y: f32,
    z: f32,
}

/// What a text slot shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Size,
    Time,
    Boards,
    Play,
    Opening,
    Status,
    Moves,
    Banner,
    Note,
    Score(usize),
    CardTitle(usize),
    CardFamily(usize),
    CardHow(usize, usize),
    CardStat(usize, usize),
}

/// A line of 3D text whose words come from the readout. The mesh lives on `mesh`.
#[derive(Component)]
struct Label {
    field: Field,
    face: Face,
    size: f32,
    align: Align,
    max_width: Option<f32>,
    material: Handle<StandardMaterial>,
    shown: String,
    mesh: Entity,
}

/// A clickable 3D button and its springy state.
#[derive(Component)]
struct Button3d {
    action: Action,
    material: Handle<StandardMaterial>,
    hovered: bool,
    pressed: bool,
    /// Lift toward the viewer (virtual pixels) and its velocity.
    lift: f32,
    lift_speed: f32,
    squash: f32,
    squash_speed: f32,
    flash: f32,
}

#[derive(Component)]
struct Ring {
    age: f32,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct EloFill(usize);

#[derive(Component)]
struct ThinkFill;

#[derive(Component)]
struct ScoreBump {
    seat: usize,
    age: f32,
    last: u32,
}

#[derive(Component)]
struct BannerPulse;

#[derive(Resource)]
struct HudPalette {
    text: Handle<StandardMaterial>,
    muted: Handle<StandardMaterial>,
    faint: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    players: [Handle<StandardMaterial>; 2],
    panel: Handle<StandardMaterial>,
    track: Handle<StandardMaterial>,
    block: Handle<Mesh>,
    ring: Handle<Mesh>,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .insert_resource(MeshPickingSettings {
                require_markers: true,
                ..default()
            })
            .add_systems(Startup, build_hud)
            .add_observer(on_over)
            .add_observer(on_out)
            .add_observer(on_press)
            .add_observer(on_release)
            .add_observer(on_click)
            .add_systems(
                Update,
                (place, show, quality, update_labels, update_bars, buttons, rings, score_bumps, banner_pulse).chain().after(ReadoutSet),
            );
    }
}

fn material(materials: &mut Assets<StandardMaterial>, colour: Color, glow: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: colour,
        emissive: colour.to_linear() * glow,
        perceptual_roughness: 0.35,
        clearcoat: 0.5,
        clearcoat_perceptual_roughness: 0.2,
        ..default()
    })
}

/// Lays out the HUD. Rows are placed with a simple cursor so widths add up exactly.
#[allow(clippy::too_many_lines)]
fn build_hud(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fonts: ResMut<Fonts>,
) {
    let palette = HudPalette {
        text: material(&mut materials, TEXT, 0.6),
        muted: material(&mut materials, MUTED, 0.5),
        faint: material(&mut materials, FAINT, 0.45),
        accent: material(&mut materials, ACCENT, 0.45),
        dark: material(&mut materials, ON_ACCENT, 0.0),
        players: [material(&mut materials, RED, 0.45), material(&mut materials, YELLOW, 0.45)],
        // Panels are flat Ember surface colour; the buttons on them carry the lighting.
        panel: materials.add(StandardMaterial {
            base_color: PANEL,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        track: material(&mut materials, RAISED, 0.0),
        block: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ring: meshes.add(Torus::new(0.46, 0.5)),
    };

    commands.spawn((
        HudCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        MeshPickingCamera,
        // MSAA on two full-resolution cameras was the biggest cost on an integrated
        // GPU; FXAA smooths edges for a fraction of it.
        Msaa::Off,
        Fxaa::default(),
        layer(),
        Transform::default(),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 1_800.0,
            shadow_maps_enabled: false,
            ..default()
        },
        layer(),
        Transform::from_xyz(-3.0, 4.0, 6.0).looking_at(Vec3::new(0.0, 0.0, -DEPTH), Vec3::Y),
    ));
    let root = commands
        .spawn((
            HudRoot,
            Transform::from_xyz(0.0, 0.0, -DEPTH).with_scale(Vec3::splat(unit())),
            Visibility::default(),
            layer(),
        ))
        .id();

    let mut hud = Hud {
        commands: &mut commands,
        fonts: &mut fonts,
        meshes: &mut meshes,
        materials: &mut materials,
        palette: &palette,
        root,
    };

    // Top: title, score, banner.
    hud.text_static("Connect Four AI", Face::Serif, 34.0, Align::Right, &palette.text, Edge::Centre, 40.0, 410.0);
    hud.text_static("train on 8x7  \u{b7}  test on 9x8", Face::Sans, 15.0, Align::Left, &palette.faint, Edge::Centre, 56.0, 410.0);
    let score_red = hud.label(Field::Score(0), Face::Bold, 34.0, Align::Right, &palette.players[0], Edge::Centre, -14.0, 362.0);
    hud.commands.entity(score_red).insert(ScoreBump { seat: 0, age: 1.0, last: 0 });
    hud.text_static(":", Face::Bold, 30.0, Align::Center, &palette.faint, Edge::Centre, 0.0, 364.0);
    let score_yellow = hud.label(Field::Score(1), Face::Bold, 34.0, Align::Left, &palette.players[1], Edge::Centre, 14.0, 362.0);
    hud.commands.entity(score_yellow).insert(ScoreBump { seat: 1, age: 1.0, last: 0 });
    hud.label(Field::Note, Face::Sans, 15.0, Align::Left, &palette.faint, Edge::Centre, 80.0, 366.0);
    let banner = hud.label(Field::Banner, Face::Bold, 20.0, Align::Center, &palette.accent, Edge::Centre, 0.0, 322.0);
    hud.commands.entity(banner).insert(BannerPulse);

    // Top right: presentation and the backup UI switch.
    hud.button(Action::Present, "present  (p)", 150.0, 32.0, Edge::Right, -87.0, 418.0);
    hud.button(Action::ToggleUi, "2D UI  (u)", 120.0, 32.0, Edge::Right, -230.0, 418.0);

    // Agent panels.
    for player in [Player::Red, Player::Yellow] {
        let seat = player.seat();
        let (edge, centre) = match player {
            Player::Red => (Edge::Left, 10.0 + SIDE_WIDTH / 2.0),
            Player::Yellow => (Edge::Right, -10.0 - SIDE_WIDTH / 2.0),
        };
        let left = centre - SIDE_WIDTH / 2.0 + 14.0;
        hud.slab(SIDE_WIDTH, 572.0, edge, centre, 37.0);
        let heading = match player {
            Player::Red => "red  \u{b7}  moves first",
            Player::Yellow => "yellow  \u{b7}  moves second",
        };
        hud.text_static(heading, Face::Bold, 17.0, Align::Left, &palette.players[seat], edge, left, 300.0);
        for (index, info) in ROSTER.iter().enumerate() {
            let y = 264.0 - 36.0 * f32_of(index);
            hud.button(Action::Pick(player, index), info.title, SIDE_WIDTH - 28.0, 30.0, edge, centre, y);
        }
        hud.label(Field::CardTitle(seat), Face::Bold, 18.0, Align::Left, &palette.text, edge, left, -36.0);
        hud.label(Field::CardFamily(seat), Face::Sans, 13.0, Align::Left, &palette.faint, edge, left, -56.0);
        for line in 0..5 {
            hud.label(Field::CardHow(seat, line), Face::Sans, 14.0, Align::Left, &palette.muted, edge, left, -80.0 - 17.0 * f32_of(line));
        }
        for line in 0..3 {
            hud.label(Field::CardStat(seat, line), Face::Sans, 14.0, Align::Left, &palette.text, edge, left, -176.0 - 18.0 * f32_of(line));
        }
        hud.block(&palette.track, SIDE_WIDTH - 28.0, 8.0, 4.0, edge, centre, -238.0, 0.0);
        let fill = hud.block(&palette.players[seat], SIDE_WIDTH - 28.0, 8.0, 6.0, edge, centre, -238.0, 1.0);
        hud.commands.entity(fill).insert(EloFill(seat));
    }

    // Bottom bar.
    hud.slab_spanning(132.0, -374.0);
    let row = -334.0;
    let mut x = 26.0;
    let mut place = |width: f32, gap: f32| {
        let centre = x + width / 2.0;
        x += width + gap;
        centre
    };
    let board_label = place(48.0, 4.0);
    let shrink = place(34.0, 4.0);
    let size = place(196.0, 4.0);
    let grow = place(34.0, 22.0);
    let time_label = place(40.0, 4.0);
    let faster = place(34.0, 4.0);
    let time = place(148.0, 4.0);
    let slower = place(34.0, 22.0);
    let opening_label = place(62.0, 4.0);
    let opening = place(170.0, 22.0);
    let boards_label = place(56.0, 4.0);
    let fewer = place(34.0, 4.0);
    let boards = place(26.0, 4.0);
    let more = place(34.0, 4.0);
    hud.text_static("board", Face::Sans, 15.0, Align::Center, &palette.faint, Edge::Left, board_label, row - 5.0);
    hud.button(Action::Shrink, "-", 34.0, 30.0, Edge::Left, shrink, row);
    hud.label(Field::Size, Face::Sans, 18.0, Align::Center, &palette.text, Edge::Left, size, row - 6.0);
    hud.button(Action::Grow, "+", 34.0, 30.0, Edge::Left, grow, row);
    hud.text_static("time", Face::Sans, 15.0, Align::Center, &palette.faint, Edge::Left, time_label, row - 5.0);
    hud.button(Action::Faster, "-", 34.0, 30.0, Edge::Left, faster, row);
    hud.label(Field::Time, Face::Sans, 18.0, Align::Center, &palette.text, Edge::Left, time, row - 6.0);
    hud.button(Action::Slower, "+", 34.0, 30.0, Edge::Left, slower, row);
    hud.text_static("opening", Face::Sans, 15.0, Align::Center, &palette.faint, Edge::Left, opening_label, row - 5.0);
    let opening_button = hud.button(Action::CycleOpening, "", 170.0, 30.0, Edge::Left, opening, row);
    hud.text_static("boards", Face::Sans, 15.0, Align::Center, &palette.faint, Edge::Left, boards_label, row - 5.0);
    hud.button(Action::FewerBoards, "-", 34.0, 30.0, Edge::Left, fewer, row);
    hud.label(Field::Boards, Face::Sans, 18.0, Align::Center, &palette.text, Edge::Left, boards, row - 6.0);
    hud.button(Action::MoreBoards, "+", 34.0, 30.0, Edge::Left, more, row);
    hud.button_label(opening_button, Field::Opening, &palette.text);
    let play = hud.button(Action::PlayPause, "", 140.0, 32.0, Edge::Right, -26.0 - 80.0 - 6.0 - 80.0 - 6.0 - 70.0, row);
    hud.button_label(play, Field::Play, &palette.dark);
    hud.button(Action::Step, "Step", 80.0, 32.0, Edge::Right, -26.0 - 80.0 - 6.0 - 40.0, row);
    hud.button(Action::Reset, "Reset", 80.0, 32.0, Edge::Right, -26.0 - 40.0, row);

    hud.label(Field::Status, Face::Sans, 18.0, Align::Left, &palette.text, Edge::Left, 26.0, -378.0);
    hud.block(&palette.track, 260.0, 10.0, 4.0, Edge::Right, -26.0 - 130.0, -372.0, 0.0);
    let think = hud.block(&palette.players[0], 260.0, 10.0, 6.0, Edge::Right, -26.0 - 130.0, -372.0, 1.0);
    hud.commands.entity(think).insert(ThinkFill);
    hud.label(Field::Moves, Face::Sans, 14.0, Align::Left, &palette.muted, Edge::Left, 26.0, -404.0);
    hud.text_static(
        "p present  \u{b7}  u 2D/3D UI  \u{b7}  space play/pause  \u{b7}  , . boards  \u{b7}  s step  \u{b7}  r reset  \u{b7}  +/- board size  \u{b7}  [ ] time  \u{b7}  o opening  \u{b7}  right-drag orbit, wheel zoom",
        Face::Sans,
        12.0,
        Align::Left,
        &palette.faint,
        Edge::Left,
        26.0,
        -428.0,
    );
    commands.insert_resource(palette);
}

fn f32_of(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

struct Hud<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    fonts: &'a mut Fonts,
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
    palette: &'a HudPalette,
    root: Entity,
}

impl Hud<'_, '_, '_> {
    fn anchored(&mut self, edge: Edge, x: f32, y: f32, z: f32) -> Entity {
        self.commands
            .spawn((
                Anchor { edge, x, y, z },
                Transform::from_xyz(x, y, z),
                Visibility::default(),
                layer(),
                ChildOf(self.root),
            ))
            .id()
    }

    /// Fixed text.
    #[allow(clippy::too_many_arguments)]
    fn text_static(&mut self, text: &str, face: Face, size: f32, align: Align, material: &Handle<StandardMaterial>, edge: Edge, x: f32, y: f32) {
        let at = self.anchored(edge, x, y, 2.0);
        let spec = Spec::new(text, face, size).align(align).flat(size < 17.0);
        if let Some(bundle) = text_bundle(self.fonts, self.meshes, &spec, material.clone(), Vec3::ZERO) {
            self.commands.spawn((bundle, layer(), Pickable::IGNORE, ChildOf(at)));
        }
    }

    /// Text that follows a readout field.
    #[allow(clippy::too_many_arguments)]
    fn label(&mut self, field: Field, face: Face, size: f32, align: Align, material: &Handle<StandardMaterial>, edge: Edge, x: f32, y: f32) -> Entity {
        let at = self.anchored(edge, x, y, 2.0);
        self.attach_label(at, field, face, size, align, material, None);
        at
    }

    #[allow(clippy::too_many_arguments)]
    fn attach_label(
        &mut self,
        at: Entity,
        field: Field,
        face: Face,
        size: f32,
        align: Align,
        material: &Handle<StandardMaterial>,
        max_width: Option<f32>,
    ) {
        let mesh = self
            .commands
            .spawn((Transform::default(), Visibility::default(), layer(), Pickable::IGNORE, ChildOf(at)))
            .id();
        self.commands.entity(at).insert(Label {
            field,
            face,
            size,
            align,
            max_width,
            material: material.clone(),
            shown: String::new(),
            mesh,
        });
    }

    /// A panel: a dark see-through slab behind a group of controls.
    fn slab(&mut self, width: f32, height: f32, edge: Edge, x: f32, y: f32) {
        let panel = self.palette.panel.clone();
        self.block(&panel, width, height, 6.0, edge, x, y, -14.0);
    }

    /// The bottom bar slab, as wide as the window (re-stretched by `place`).
    fn slab_spanning(&mut self, height: f32, y: f32) {
        let panel = self.palette.panel.clone();
        let at = self.anchored(Edge::Centre, 0.0, y, -14.0);
        self.commands.entity(at).insert(Spanning { height });
        self.commands.spawn((
            Mesh3d(self.palette.block.clone()),
            MeshMaterial3d(panel),
            Transform::from_scale(Vec3::new(1.0, height, 6.0)),
            layer(),
            ChildOf(at),
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn block(&mut self, material: &Handle<StandardMaterial>, width: f32, height: f32, depth: f32, edge: Edge, x: f32, y: f32, z: f32) -> Entity {
        let at = self.anchored(edge, x, y, z);
        self.commands.entity(at).insert((
            Mesh3d(self.palette.block.clone()),
            MeshMaterial3d(material.clone()),
            BlockSize(Vec3::new(width, height, depth)),
        ));
        at
    }

    /// A button: its own material (for hover and flash), a fixed caption unless one is
    /// added with `button_label`.
    #[allow(clippy::too_many_arguments)]
    fn button(&mut self, action: Action, caption: &str, width: f32, height: f32, edge: Edge, x: f32, y: f32) -> Entity {
        let base = if action == Action::PlayPause { ACCENT } else { RAISED };
        let material = self.materials.add(StandardMaterial {
            base_color: base,
            perceptual_roughness: 0.3,
            clearcoat: 0.8,
            clearcoat_perceptual_roughness: 0.12,
            ..default()
        });
        let at = self.anchored(edge, x, y, 0.0);
        self.commands.entity(at).insert((
            Button3d {
                action,
                material: material.clone(),
                hovered: false,
                pressed: false,
                lift: 0.0,
                lift_speed: 0.0,
                squash: 1.0,
                squash_speed: 0.0,
                flash: 0.0,
            },
            Size2(Vec2::new(width, height)),
        ));
        self.commands.spawn((
            Mesh3d(self.palette.block.clone()),
            MeshMaterial3d(material),
            Transform::from_scale(Vec3::new(width, height, 10.0)),
            Pickable::default(),
            layer(),
            ChildOf(at),
        ));
        if !caption.is_empty() {
            // Single symbols (- and +) get a bigger, bolder glyph.
            let (face, size) = if caption.chars().count() == 1 { (Face::Bold, 24.0) } else { (Face::Sans, 15.0) };
            let spec = Spec::new(caption, face, size).align(Align::Center).fit(width - 12.0).flat(true);
            let colour = if action == Action::PlayPause { self.palette.dark.clone() } else { self.palette.text.clone() };
            let drop = if caption.chars().count() == 1 { -8.0 } else { -5.0 };
            if let Some(bundle) = text_bundle(self.fonts, self.meshes, &spec, colour, Vec3::new(0.0, drop, 6.0)) {
                self.commands.spawn((bundle, layer(), Pickable::IGNORE, ChildOf(at)));
            }
        }
        at
    }

    /// Gives a button a caption that follows a readout field.
    fn button_label(&mut self, button: Entity, field: Field, material: &Handle<StandardMaterial>) {
        let at = self
            .commands
            .spawn((Transform::from_xyz(0.0, -5.5, 6.0), Visibility::default(), layer(), ChildOf(button)))
            .id();
        let face = if field == Field::Play { Face::Bold } else { Face::Sans };
        self.attach_label(at, field, face, 16.0, Align::Center, material, Some(150.0));
    }
}

/// A plain block's size (it is a scaled unit cube).
#[derive(Component)]
struct BlockSize(Vec3);

#[derive(Component)]
struct Size2(Vec2);

/// The bottom bar slab: stretched to the window width.
#[derive(Component)]
struct Spanning {
    height: f32,
}

/// Window width in virtual pixels.
fn width_px(window: &Window) -> f32 {
    TALL * window.width().max(1.0) / window.height().max(1.0)
}

/// Positions every anchored element for the window's shape.
fn place(
    windows: Query<&Window>,
    mut anchored: Query<(&Anchor, &mut Transform, Option<&BlockSize>, Option<&Spanning>, &Children)>,
    mut slabs: Query<&mut Transform, (Without<Anchor>, With<Mesh3d>)>,
    mut plain: Query<(&Anchor, &mut Transform, Option<&BlockSize>), Without<Children>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let wide = width_px(window);
    let edge_x = |edge: Edge| match edge {
        Edge::Left => -wide / 2.0,
        Edge::Centre => 0.0,
        Edge::Right => wide / 2.0,
    };
    for (anchor, mut transform, size, spanning, children) in &mut anchored {
        transform.translation.x = edge_x(anchor.edge) + anchor.x;
        transform.translation.y = anchor.y;
        if let Some(size) = size {
            transform.scale = size.0;
        }
        if let Some(spanning) = spanning {
            for &child in children {
                if let Ok(mut slab) = slabs.get_mut(child) {
                    slab.scale = Vec3::new(wide - 20.0, spanning.height, 6.0);
                }
            }
        }
    }
    for (anchor, mut transform, size) in &mut plain {
        transform.translation.x = edge_x(anchor.edge) + anchor.x;
        transform.translation.y = anchor.y;
        if let Some(size) = size {
            transform.scale = size.0;
        }
    }
}

/// HUD in the arena with the 3D style; flat UI otherwise (see `ui.rs`).
fn show(
    style: Res<UiStyle>,
    mode: Res<State<Mode>>,
    mut roots: Query<&mut Visibility, With<HudRoot>>,
    mut cameras: Query<&mut Camera, With<HudCamera>>,
) {
    let shown = style.solid && *mode.get() == Mode::Arena;
    for mut visibility in &mut roots {
        let wanted = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    for mut camera in &mut cameras {
        if camera.is_active != shown {
            camera.is_active = shown;
        }
    }
}

/// Backup mode is also "lite": no bloom and no shadows on the main scene.
fn quality(
    mut commands: Commands,
    style: Res<UiStyle>,
    cameras: Query<(Entity, Has<Bloom>), (With<Camera3d>, Without<HudCamera>)>,
    mut lights: Query<&mut DirectionalLight, Without<RenderLayers>>,
) {
    if !style.is_changed() {
        return;
    }
    for (camera, bloom) in &cameras {
        if style.solid && !bloom {
            commands.entity(camera).insert(Bloom {
                intensity: 0.12,
                ..Bloom::NATURAL
            });
        } else if !style.solid && bloom {
            commands.entity(camera).remove::<Bloom>();
        }
    }
    for mut light in &mut lights {
        light.shadow_maps_enabled = style.solid;
    }
}

/// Greedy word wrap by an average character width.
fn wrap(text: &str, chars: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > chars {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn field_text(readout: &Readout, field: Field) -> String {
    match field {
        Field::Size => readout.size.clone(),
        Field::Time => readout.time.clone(),
        Field::Boards => readout.boards.clone(),
        Field::Play => readout.play.clone(),
        Field::Opening => readout.opening.clone(),
        Field::Status => readout.status.clone(),
        Field::Moves => readout.moves.clone(),
        Field::Banner => readout.banner.clone(),
        Field::Note => readout.note.clone(),
        Field::Score(seat) => readout.score[seat].to_string(),
        Field::CardTitle(seat) => readout.cards[seat].title.clone(),
        Field::CardFamily(seat) => readout.cards[seat].family.clone(),
        Field::CardHow(seat, line) => wrap(&readout.cards[seat].how, 36).get(line).cloned().unwrap_or_default(),
        Field::CardStat(seat, line) => readout.cards[seat].stats[line].clone(),
    }
}

/// Rebuilds a label's mesh when its words change.
/// Rebuilds a label's mesh when its words change, at most about 12 times a second so
/// very fast games do not spend their time re-meshing text.
#[allow(clippy::too_many_arguments)]
fn update_labels(
    mut commands: Commands,
    time: Res<Time>,
    readout: Res<Readout>,
    style: Res<UiStyle>,
    mut fonts: ResMut<Fonts>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut labels: Query<&mut Label>,
    parents: Query<&ChildOf>,
    mut last: Local<f32>,
) {
    let now = time.elapsed_secs();
    if !style.solid || (now - *last < 0.08 && !style.is_changed()) {
        return;
    }
    *last = now;
    for mut label in &mut labels {
        let wanted = field_text(&readout, label.field);
        if wanted == label.shown {
            continue;
        }
        let size = if label.field == Field::Banner && readout.banner_result { 24.0 } else { label.size };
        let mut spec = Spec::new(&wanted, label.face, size).align(label.align).flat(size < 17.0);
        if let Some(width) = label.max_width {
            spec = spec.fit(width);
        }
        if label.field == Field::Moves {
            spec = spec.fit(1300.0);
        }
        // A fresh child per change (like the fixed text), rather than swapping the
        // mesh on an existing entity.
        let parent = commands.entity(label.mesh).id();
        let anchor = parents.get(parent).map(ChildOf::parent).ok();
        commands.entity(label.mesh).despawn();
        if let Some(anchor) = anchor {
            let child = match text_bundle(&mut fonts, &mut meshes, &spec, label.material.clone(), Vec3::ZERO) {
                Some(bundle) => commands.spawn((bundle, layer(), Pickable::IGNORE, ChildOf(anchor))).id(),
                None => commands.spawn((Transform::default(), Visibility::default(), layer(), ChildOf(anchor))).id(),
            };
            label.mesh = child;
        }
        label.shown = wanted;
    }
}

/// Elo bars and the think bar: a block scaled from its left end.
fn update_bars(
    readout: Res<Readout>,
    windows: Query<&Window>,
    palette: Res<HudPalette>,
    mut elo: Query<(&EloFill, &Anchor, &mut Transform), Without<ThinkFill>>,
    mut think: Query<(&Anchor, &mut Transform, &mut MeshMaterial3d<StandardMaterial>), With<ThinkFill>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let wide = width_px(window);
    let edge_x = |edge: Edge| match edge {
        Edge::Left => -wide / 2.0,
        Edge::Centre => 0.0,
        Edge::Right => wide / 2.0,
    };
    for (fill, anchor, mut transform) in &mut elo {
        let full = SIDE_WIDTH - 28.0;
        let width = full * readout.cards[fill.0].elo_share;
        transform.scale.x = width.max(0.5);
        transform.translation.x = edge_x(anchor.edge) + anchor.x - full / 2.0 + width / 2.0;
    }
    for (anchor, mut transform, mut material) in &mut think {
        let full = 260.0;
        let width = full * readout.think;
        transform.scale.x = width.max(0.01);
        transform.translation.x = edge_x(anchor.edge) + anchor.x - full / 2.0 + width / 2.0;
        let wanted = &palette.players[readout.to_move.seat()];
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

fn on_over(mut event: On<PointerOver>, mut buttons: Query<&mut Button3d>, parents: Query<&ChildOf>) {
    if let Some(mut button) = owner(event.entity, &parents).and_then(|entity| buttons.get_mut(entity).ok()) {
        button.hovered = true;
        event.propagate(false);
    }
}

fn on_out(mut event: On<PointerOut>, mut buttons: Query<&mut Button3d>, parents: Query<&ChildOf>) {
    if let Some(mut button) = owner(event.entity, &parents).and_then(|entity| buttons.get_mut(entity).ok()) {
        button.hovered = false;
        button.pressed = false;
        event.propagate(false);
    }
}

fn on_press(mut event: On<PointerPress>, mut buttons: Query<&mut Button3d>, parents: Query<&ChildOf>) {
    debug!("hud press on {:?}", event.entity);
    if event.button != PointerButton::Primary {
        return;
    }
    if let Some(mut button) = owner(event.entity, &parents).and_then(|entity| buttons.get_mut(entity).ok()) {
        button.pressed = true;
        event.propagate(false);
    }
}

fn on_release(mut event: On<PointerRelease>, mut buttons: Query<&mut Button3d>, parents: Query<&ChildOf>) {
    if let Some(mut button) = owner(event.entity, &parents).and_then(|entity| buttons.get_mut(entity).ok()) {
        button.pressed = false;
        event.propagate(false);
    }
}

fn on_click(
    mut event: On<PointerClick>,
    mut commands: Commands,
    mut buttons: Query<(&mut Button3d, &Size2)>,
    parents: Query<&ChildOf>,
    mut pending: ResMut<Pending>,
    palette: Res<HudPalette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    debug!("hud click on {:?}", event.entity);
    if event.button != PointerButton::Primary {
        return;
    }
    let Some(entity) = owner(event.entity, &parents) else {
        return;
    };
    let Ok((mut button, size)) = buttons.get_mut(entity) else {
        return;
    };
    event.propagate(false);
    pending.0.push(button.action);
    // Impact: a kick back into the panel, a flash, and a ring of light.
    button.lift_speed -= 260.0;
    button.squash_speed -= 6.0;
    button.flash = 1.0;
    let ring = materials.add(StandardMaterial {
        base_color: ACCENT.with_alpha(0.9),
        emissive: ACCENT.to_linear() * 4.0,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Ring { age: 0.0, material: ring.clone() },
        Mesh3d(palette.ring.clone()),
        MeshMaterial3d(ring),
        Transform::from_xyz(0.0, 0.0, 6.0)
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
            .with_scale(Vec3::new(size.0.x, 1.0, size.0.y)),
        layer(),
        ChildOf(entity),
    ));
}

/// The button entity for a hit: the hit itself or its parent (the block is a child).
fn owner(hit: Entity, parents: &Query<&ChildOf>) -> Option<Entity> {
    parents.get(hit).ok().map(ChildOf::parent).or(Some(hit))
}

/// Springs, colours and flashes for every button.
fn buttons(
    time: Res<Time>,
    settings: Res<Settings>,
    mut buttons: Query<(&mut Button3d, &Anchor, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (mut button, anchor, mut transform) in &mut buttons {
        let selected = match button.action {
            Action::Pick(player, agent) => (settings.agent[player.seat()] == agent).then_some(player),
            _ => None,
        };
        let target_lift = match (button.pressed, button.hovered) {
            (true, _) => -4.0,
            (false, true) => 9.0,
            (false, false) => if selected.is_some() { 4.0 } else { 0.0 },
        };
        let target_squash = if button.pressed { 0.93 } else if button.hovered { 1.04 } else { 1.0 };
        // A stiff, slightly bouncy spring.
        let pull = (target_lift - button.lift) * 320.0 - button.lift_speed * 18.0;
        button.lift_speed += pull * dt;
        button.lift += button.lift_speed * dt;
        let pull = (target_squash - button.squash) * 320.0 - button.squash_speed * 16.0;
        button.squash_speed += pull * dt;
        button.squash += button.squash_speed * dt;
        button.flash = (button.flash - dt * 3.5).max(0.0);
        transform.translation.z = anchor.z + button.lift;
        transform.scale = Vec3::splat(button.squash);
        // Hover tilts the block a touch toward the light.
        transform.rotation = Quat::from_rotation_x(if button.hovered { -0.12 } else { 0.0 });

        let base = match (button.action, selected) {
            (Action::PlayPause, _) => ACCENT,
            (_, Some(Player::Red)) => RED.mix(&RAISED, 0.45),
            (_, Some(Player::Yellow)) => YELLOW.mix(&RAISED, 0.45),
            _ if button.hovered => HOVER,
            _ => RAISED,
        };
        let glow = button.flash * 2.5 + if button.hovered { 0.15 } else { 0.0 } + if selected.is_some() { 0.25 } else { 0.0 };
        // Touch the material only when it really changes: every changed material is
        // re-prepared on the GPU, and doing that for every button every frame piled up.
        let colour = base.mix(&BORDER, 0.0);
        let emissive = colour.to_linear() * ((glow * 50.0).round() / 50.0);
        let stale = materials.get(&button.material).is_some_and(|material| material.base_color != colour || material.emissive != emissive);
        if stale && let Some(mut material) = materials.get_mut(&button.material) {
            material.base_color = colour;
            material.emissive = emissive;
        }
    }
}

fn rings(
    mut commands: Commands,
    time: Res<Time>,
    mut rings: Query<(Entity, &mut Ring, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut ring, mut transform) in &mut rings {
        ring.age += time.delta_secs();
        let t = (ring.age / 0.45).min(1.0);
        let grow = 1.0 + t * 0.6;
        transform.scale.x *= 1.0 + time.delta_secs() * 0.5;
        transform.scale.z *= 1.0 + time.delta_secs() * 0.9;
        transform.scale.y = grow;
        if let Some(mut material) = materials.get_mut(&ring.material) {
            material.base_color = material.base_color.with_alpha(0.9 * (1.0 - t));
            material.emissive = ACCENT.to_linear() * 4.0 * (1.0 - t);
        }
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// A side's score number jumps when it goes up.
fn score_bumps(time: Res<Time>, readout: Res<Readout>, mut scores: Query<(&mut ScoreBump, &mut Transform)>) {
    for (mut bump, mut transform) in &mut scores {
        let now = readout.score[bump.seat];
        if now != bump.last {
            bump.last = now;
            bump.age = 0.0;
        }
        bump.age += time.delta_secs();
        let scale = 1.0 + 0.6 * (-bump.age * 6.0).exp() * (bump.age * 16.0).cos().abs();
        transform.scale = Vec3::splat(scale);
    }
}

/// The banner breathes while it announces a result.
fn banner_pulse(time: Res<Time>, readout: Res<Readout>, palette: Res<HudPalette>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow = if readout.banner_result { 0.6 + (time.elapsed_secs() * 6.0).sin().abs() * 1.6 } else { 0.45 };
    let emissive = ACCENT.to_linear() * ((glow * 20.0).round() / 20.0);
    let stale = materials.get(&palette.accent).is_some_and(|material| material.emissive != emissive);
    if stale && let Some(mut material) = materials.get_mut(&palette.accent) {
        material.emissive = emissive;
    }
}
