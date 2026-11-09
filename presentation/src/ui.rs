//! The arena's screen UI, all flat 2D text and panels: the score across the top, an
//! agent picker and card down each side, and the match controls along the bottom.
//!
//! It is animated so clicks feel like they do something:
//! - panels slide in whenever the arena opens (and again after the slides);
//! - buttons spring up on hover, squash when pressed and send out a ripple on click;
//! - a value that changes (board size, time, a score) pops and flashes orange;
//! - each point scored floats a "+1" off the scoreboard;
//! - a win flashes the screen edge in the winner's colour and the result banner pulses;
//! - switching agent slides the new agent's card in.
//!
//! The panels' measured sizes go to `scene::UiMargins`, so the camera always frames the
//! boards in the space between them.

use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button};

use crate::effects::Won;
use crate::game::{Action, Pending, Player, Settings};
use crate::present::Mode;
use crate::readout::{Readout, ReadoutSet};
use crate::roster::ROSTER;
use crate::scene::UiMargins;
use crate::theme::{ACCENT, BG, BORDER, FAINT, HOVER, MUTED, ON_ACCENT, PANEL, PRESS, RAISED, RED, TEXT, YELLOW, bold_font, title_font, ui_font};

const BUTTON: Color = RAISED;
const SIDE_WIDTH: f32 = 320.0;
/// Space between the window edge and the panels.
const PAD: f32 = 12.0;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IntroClock>()
            .add_systems(Startup, build_ui)
            .add_observer(on_activate)
            .add_systems(OnEnter(Mode::Arena), |mut clock: ResMut<IntroClock>| clock.0 = 0.0)
            .add_systems(
                Update,
                (
                    style_buttons,
                    ripples,
                    update_values,
                    update_cards,
                    update_bars,
                    update_score,
                    floaters,
                    pops,
                    intro,
                    banner_glow,
                    win_flash,
                    measure,
                )
                    .after(ReadoutSet)
                    .run_if(in_state(Mode::Arena)),
            )
            .add_systems(Update, (show_ui, fit_ui));
    }
}

/// The whole overlay, hidden while presenting.
#[derive(Component)]
struct UiRoot;

fn show_ui(mode: Res<State<Mode>>, mut roots: Query<&mut Node, With<UiRoot>>) {
    let wanted = if *mode.get() == Mode::Arena { Display::Flex } else { Display::None };
    for mut node in &mut roots {
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// The panels whose size keeps the boards clear of them.
#[derive(Component)]
struct TopBar;
#[derive(Component)]
struct SidePanel;
#[derive(Component)]
struct BottomBar;

/// A text that shows one readout field.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Value {
    Size,
    Time,
    Boards,
    Opening,
    Play,
    Status,
    Moves,
    Note,
    CardTitle(usize),
    CardFamily(usize),
    CardHow(usize),
    CardStats(usize),
}

impl Value {
    fn read(self, readout: &Readout) -> String {
        match self {
            Self::Size => readout.size.clone(),
            Self::Time => readout.time.clone(),
            Self::Boards => readout.boards.clone(),
            Self::Opening => readout.opening.clone(),
            Self::Play => readout.play.clone(),
            Self::Status => readout.status.clone(),
            Self::Moves => readout.moves.clone(),
            Self::Note => readout.note.clone(),
            Self::CardTitle(seat) => readout.cards[seat].title.clone(),
            Self::CardFamily(seat) => readout.cards[seat].family.clone(),
            Self::CardHow(seat) => readout.cards[seat].how.clone(),
            Self::CardStats(seat) => readout.cards[seat].stats.join("\n"),
        }
    }
}

#[derive(Component)]
struct Banner {
    age: f32,
    result: bool,
}
#[derive(Component)]
struct ScoreText(Player);
/// Holds a score and the "+1"s that float off it.
#[derive(Component)]
struct ScoreBox(Player);
#[derive(Component)]
struct ThinkFill;
/// The Elo bar; `shown` eases toward the agent's value.
#[derive(Component)]
struct EloFill {
    player: Player,
    shown: f32,
}
/// An agent card; slides in from its side when the agent changes.
#[derive(Component)]
struct CardBox {
    player: Player,
    age: f32,
    title: String,
}

/// A text that pops (grows and flashes orange) when its value changes.
#[derive(Component)]
struct Pop {
    age: f32,
    base: Color,
    strength: f32,
}

impl Pop {
    const fn new(base: Color, strength: f32) -> Self {
        Self { age: 10.0, base, strength }
    }
}

/// A button's spring: scale and its velocity, plus a flash after a click.
#[derive(Component, Default)]
struct Juice {
    scale: f32,
    velocity: f32,
    flash: f32,
}

/// A ring that grows out of a clicked button and fades.
#[derive(Component)]
struct Ripple(f32);

/// A "+1" rising off the scoreboard.
#[derive(Component)]
struct Floater {
    age: f32,
    colour: Color,
}

/// A glow round the window edge when a game is won.
#[derive(Component)]
struct EdgeFlash {
    age: f32,
    colour: Color,
}

/// Slides a panel in from `from` (pixels) when the arena opens.
#[derive(Component)]
struct Enter {
    from: Vec2,
    delay: f32,
}

/// Seconds since the arena last opened.
#[derive(Resource, Default)]
struct IntroClock(f32);

const fn colour(player: Player) -> Color {
    match player {
        Player::Red => RED,
        Player::Yellow => YELLOW,
    }
}

fn font(size: f32) -> TextFont {
    TextFont {
        font: ui_font(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn bold(size: f32) -> TextFont {
    // A real semibold face: the fonts are loaded from files, so weights are not synthesised.
    TextFont {
        font: bold_font(),
        ..font(size)
    }
}

fn label(text: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (Text::new(text), font(size), TextColor(colour))
}

/// A small heading over a control, in capitals.
fn caption(text: &str) -> impl Bundle {
    (Text::new(text.to_uppercase()), bold(13.0), TextColor(FAINT))
}

fn button_node(width: Val, height: f32) -> Node {
    Node {
        width,
        min_height: px(height),
        padding: UiRect::axes(px(12), px(4)),
        margin: UiRect::all(px(2)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(9)),
        ..default()
    }
}

fn button(action: Action, text: &str, width: Val) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        Juice::default(),
        action,
        button_node(width, 34.0),
        BorderColor::all(BORDER),
        BackgroundColor(BUTTON),
        children![(Text::new(text), font(16.0), TextColor(TEXT), Pickable::IGNORE)],
    )
}

/// A button whose words come from the readout.
fn value_button(action: Action, value: Value, width: Val, height: f32, size: f32, ink: Color) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        Juice::default(),
        action,
        button_node(width, height),
        BorderColor::all(BORDER),
        BackgroundColor(BUTTON),
        children![(Text::new(""), bold(size), TextColor(ink), value, Pickable::IGNORE)],
    )
}

fn panel(node: Node) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(14)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(12)),
            ..node
        },
        BackgroundColor(PANEL),
        BorderColor::all(BORDER),
        BoxShadow::new(Color::BLACK.with_alpha(0.45), px(0), px(6), px(0), px(18)),
    )
}

fn build_ui(mut commands: Commands) {
    commands
        .spawn((
            UiRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(px(PAD)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // The edge glow sits under everything and never takes clicks.
            root.spawn((
                EdgeFlash { age: 10.0, colour: ACCENT },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: px(0),
                    bottom: px(0),
                    border: UiRect::all(px(7)),
                    ..default()
                },
                BorderColor::all(Color::NONE),
                Pickable::IGNORE,
            ));
            top_bar(root);
            root.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::FlexStart,
                    margin: UiRect::vertical(px(10)),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|middle| {
                for player in [Player::Red, Player::Yellow] {
                    agent_panel(middle, player);
                }
            });
            root.spawn((
                BottomBar,
                Enter {
                    from: Vec2::new(0.0, 260.0),
                    delay: 0.15,
                },
                panel(Node { row_gap: px(8), ..default() }),
            ))
            .with_children(controls);
        });
}

fn top_bar(root: &mut ChildSpawnerCommands<'_>) {
    root.spawn((
        TopBar,
        Enter {
            from: Vec2::new(0.0, -140.0),
            delay: 0.0,
        },
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|top| {
        // Left: the name of the thing.
        top.spawn(Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|left| {
            left.spawn((
                Text::new("Connect Four AI"),
                TextFont {
                    font: title_font(),
                    font_size: FontSize::Px(38.0),
                    weight: FontWeight::THIN,
                    ..default()
                },
                TextColor(TEXT),
            ));
            left.spawn(label("Trained on 8 \u{d7} 7, tested on 9 \u{d7} 8", 16.0, MUTED));
        });
        // Middle: the score and who is playing.
        top.spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|middle| {
            middle
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(16),
                    ..default()
                })
                .with_children(|score| {
                    score.spawn(caption("Red"));
                    score_box(score, Player::Red);
                    score.spawn(label(":", 36.0, FAINT));
                    score_box(score, Player::Yellow);
                    score.spawn(caption("Yellow"));
                });
            middle.spawn((
                Text::new(""),
                bold(21.0),
                TextColor(TEXT),
                Banner { age: 10.0, result: false },
            ));
            middle.spawn((label("", 15.0, FAINT), Value::Note));
        });
        // Right: the way into the slides.
        top.spawn(Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .with_children(|right| {
            right.spawn(button(Action::Present, "Slides  (P)", px(150)));
        });
    });
}

fn score_box(parent: &mut ChildSpawnerCommands<'_>, player: Player) {
    parent
        .spawn((ScoreBox(player), Node { min_width: px(48), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|slot| {
            slot.spawn((Text::new("0"), bold(46.0), TextColor(colour(player)), ScoreText(player), Pop::new(colour(player), 0.6)));
        });
}

fn agent_panel(parent: &mut ChildSpawnerCommands<'_>, player: Player) {
    let (heading, side) = match player {
        Player::Red => ("Red  \u{b7}  moves first", -1.0),
        Player::Yellow => ("Yellow  \u{b7}  moves second", 1.0),
    };
    parent
        .spawn((
            SidePanel,
            Enter {
                from: Vec2::new(side * (SIDE_WIDTH + 60.0), 0.0),
                delay: 0.08,
            },
            panel(Node {
                width: px(SIDE_WIDTH),
                row_gap: px(1),
                ..default()
            }),
        ))
        .with_children(|panel| {
            panel.spawn((Text::new(heading), bold(19.0), TextColor(colour(player))));
            panel.spawn((caption("Choose an agent"), Node { margin: UiRect::vertical(px(4)), ..default() }));
            for (agent, info) in ROSTER.iter().enumerate() {
                panel.spawn(button(Action::Pick(player, agent), info.title, percent(100)));
            }
            let seat = player.seat();
            panel
                .spawn((
                    CardBox {
                        player,
                        age: 10.0,
                        title: String::new(),
                    },
                    Node {
                        flex_direction: FlexDirection::Column,
                        margin: UiRect::top(px(10)),
                        row_gap: px(4),
                        ..default()
                    },
                ))
                .with_children(|card| {
                    card.spawn((Text::new(""), bold(20.0), TextColor(TEXT), Value::CardTitle(seat)));
                    card.spawn((label("", 14.0, FAINT), Value::CardFamily(seat)));
                    card.spawn((label("", 15.0, MUTED), Value::CardHow(seat)));
                    card.spawn((label("", 15.0, TEXT), Value::CardStats(seat)));
                    card.spawn((
                        Node {
                            width: percent(100),
                            height: px(8),
                            margin: UiRect::top(px(2)),
                            border_radius: BorderRadius::all(px(4)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|track| {
                        track.spawn((
                            Node {
                                width: percent(0),
                                height: percent(100),
                                border_radius: BorderRadius::all(px(4)),
                                ..default()
                            },
                            BackgroundColor(colour(player)),
                            EloFill { player, shown: 0.0 },
                        ));
                    });
                });
        });
}

/// A labelled control: a caption over its buttons.
fn group(parent: &mut ChildSpawnerCommands<'_>, title: &str, body: impl FnOnce(&mut ChildSpawnerCommands<'_>)) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        })
        .with_children(|group| {
            group.spawn(caption(title));
            group
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    ..default()
                })
                .with_children(body);
        });
}

/// Minus, the current value, plus.
fn stepper(row: &mut ChildSpawnerCommands<'_>, less: Action, value: Value, more: Action, width: f32) {
    row.spawn(button(less, "\u{2212}", px(38)));
    row.spawn((
        Node {
            width: px(width),
            height: px(36),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(BG.with_alpha(0.7)),
    ))
    .with_children(|chip| {
        chip.spawn((Text::new(""), bold(19.0), TextColor(TEXT), value, Pop::new(TEXT, 0.25)));
    });
    row.spawn(button(more, "+", px(38)));
}

fn controls(bar: &mut ChildSpawnerCommands<'_>) {
    bar.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::FlexEnd,
        column_gap: px(22),
        row_gap: px(8),
        ..default()
    })
    .with_children(|line| {
        group(line, "Board size", |row| stepper(row, Action::Shrink, Value::Size, Action::Grow, 236.0));
        group(line, "Time per move", |row| stepper(row, Action::Faster, Value::Time, Action::Slower, 84.0));
        group(line, "Boards at once", |row| stepper(row, Action::FewerBoards, Value::Boards, Action::MoreBoards, 64.0));
        group(line, "Starting position", |row| {
            row.spawn(value_button(Action::CycleOpening, Value::Opening, px(170), 36.0, 16.0, TEXT));
        });
        line.spawn(Node { flex_grow: 1.0, ..default() });
        line.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(4),
            ..default()
        })
        .with_children(|actions| {
            actions.spawn(value_button(Action::PlayPause, Value::Play, px(160), 46.0, 20.0, ON_ACCENT));
            actions.spawn(button(Action::Step, "One move", px(112)));
            actions.spawn(button(Action::Reset, "Reset", px(86)));
        });
    });
    bar.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(12),
        ..default()
    })
    .with_children(|line| {
        line.spawn((label("", 19.0, TEXT), Value::Status, Node { flex_grow: 1.0, ..default() }));
        line.spawn(caption("Thinking"));
        line.spawn((
            Node {
                width: px(240),
                height: px(10),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(BUTTON),
        ))
        .with_children(|track| {
            track.spawn((
                Node {
                    width: percent(0),
                    height: percent(100),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(ACCENT),
                ThinkFill,
            ));
        });
    });
    bar.spawn((label("", 15.0, MUTED), Value::Moves));
    bar.spawn(label(
        "Space  play / pause     S  one move     R  reset     \u{2212} +  board size     [ ]  time     , .  boards     O  start position     P  slides     right-drag to orbit, scroll to zoom",
        14.0,
        FAINT,
    ));
}

fn on_activate(
    event: On<Activate>,
    mut commands: Commands,
    mut buttons: Query<(&Action, &mut Juice)>,
    mut pending: ResMut<Pending>,
) {
    let Ok((&action, mut juice)) = buttons.get_mut(event.entity) else {
        return;
    };
    pending.0.push(action);
    juice.flash = 1.0;
    juice.velocity -= 3.0;
    let ring = match action {
        Action::Pick(player, _) => colour(player),
        _ => ACCENT,
    };
    commands.spawn((
        Ripple(0.0),
        Node {
            position_type: PositionType::Absolute,
            left: px(-1),
            right: px(-1),
            top: px(-1),
            bottom: px(-1),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BorderColor::all(ring),
        Pickable::IGNORE,
        ChildOf(event.entity),
    ));
}

/// Colours ease toward their state, and each button springs toward its size: up on
/// hover, down while pressed, wobbling after a click.
#[allow(clippy::type_complexity)]
fn style_buttons(
    time: Res<Time>,
    settings: Res<Settings>,
    mut buttons: Query<(
        &Action,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        &mut Juice,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut UiTransform,
    )>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let blend = 1.0 - (-dt * 16.0).exp();
    for (action, hovered, pressed, disabled, mut juice, mut background, mut border, mut transform) in &mut buttons {
        let hover = hovered.get() && !disabled;
        let selected = match *action {
            Action::Pick(player, agent) => (settings.agent[player.seat()] == agent).then_some(colour(player)),
            _ => None,
        };
        let primary = *action == Action::PlayPause;
        // Play is Ember's filled orange primary button.
        let base = match (primary, pressed, hover) {
            (true, true, _) => ACCENT.mix(&PRESS, 0.25),
            (true, false, true) => ACCENT.mix(&TEXT, 0.2),
            (true, false, false) => ACCENT,
            (false, true, _) => PRESS,
            (false, false, true) => HOVER,
            (false, false, false) => BUTTON,
        };
        let (fill, edge) = match selected {
            Some(tint) => (tint.mix(&base, 0.6), tint),
            None if primary => (base, ACCENT),
            None if hover => (base, ACCENT.mix(&BORDER, 0.5)),
            None => (base, BORDER),
        };
        juice.flash = (juice.flash - dt * 3.0).max(0.0);
        let fill = fill.mix(&TEXT, juice.flash * 0.35);
        background.0 = background.0.mix(&fill, blend);
        *border = BorderColor::all(edge);

        if juice.scale == 0.0 {
            juice.scale = 1.0;
        }
        let target = if pressed {
            0.93
        } else if hover {
            1.05
        } else {
            1.0
        };
        let force = (target - juice.scale) * 520.0 - juice.velocity * 20.0;
        juice.velocity += force * dt;
        juice.scale += juice.velocity * dt;
        let scale = Vec2::splat(juice.scale);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

fn ripples(mut commands: Commands, time: Res<Time>, mut rings: Query<(Entity, &mut Ripple, &mut UiTransform, &mut BorderColor)>) {
    for (entity, mut ring, mut transform, mut border) in &mut rings {
        ring.0 += time.delta_secs();
        let t = (ring.0 / 0.5).min(1.0);
        let grow = 1.0 - (1.0 - t).powi(3);
        transform.scale = Vec2::new(1.0 + grow * 0.18, 1.0 + grow * 0.5);
        let colour = border.top.with_alpha((1.0 - t) * 0.9);
        *border = BorderColor::all(colour);
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn set(text: &mut Text, value: &str) -> bool {
    let changed = text.0 != value;
    if changed {
        value.clone_into(&mut text.0);
    }
    changed
}

/// Every readout text; values that change pop.
fn update_values(readout: Res<Readout>, mut texts: Query<(&Value, &mut Text, Option<&mut Pop>), Without<Banner>>) {
    if !readout.is_changed() {
        return;
    }
    for (value, mut text, pop) in &mut texts {
        if set(&mut text, &value.read(&readout))
            && let Some(mut pop) = pop
        {
            pop.age = 0.0;
        }
    }
}

fn update_cards(time: Res<Time>, readout: Res<Readout>, mut cards: Query<(&mut CardBox, &mut UiTransform)>) {
    for (mut card, mut transform) in &mut cards {
        let title = &readout.cards[card.player.seat()].title;
        if card.title != *title {
            // The first fill at startup is not a change.
            card.age = if card.title.is_empty() { 10.0 } else { 0.0 };
            title.clone_into(&mut card.title);
        }
        card.age += time.delta_secs();
        let t = (card.age / 0.45).min(1.0);
        let rest = (1.0 - t).powi(3);
        let side = if card.player == Player::Red { -1.0 } else { 1.0 };
        let translation = Val2::px(side * 70.0 * rest, 0.0);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}

fn update_bars(
    time: Res<Time>,
    readout: Res<Readout>,
    mut elo: Query<(&mut EloFill, &mut Node), Without<ThinkFill>>,
    mut think: Query<(&mut Node, &mut BackgroundColor), With<ThinkFill>>,
) {
    let blend = 1.0 - (-time.delta_secs() * 6.0).exp();
    for (mut bar, mut node) in &mut elo {
        let wanted = readout.cards[bar.player.seat()].elo_share * 100.0;
        bar.shown += (wanted - bar.shown) * blend;
        node.width = percent(bar.shown);
    }
    for (mut node, mut background) in &mut think {
        node.width = percent(readout.think * 100.0);
        background.0 = colour(readout.to_move);
    }
}

fn update_score(
    mut commands: Commands,
    readout: Res<Readout>,
    mut scores: Query<(&ScoreText, &mut Text, &mut Pop)>,
    boxes: Query<(Entity, &ScoreBox)>,
) {
    for (score, mut text, mut pop) in &mut scores {
        let old: u32 = text.0.parse().unwrap_or(0);
        let new = readout.score[score.0.seat()];
        if !set(&mut text, &new.to_string()) {
            continue;
        }
        pop.age = 0.0;
        if new <= old {
            continue;
        }
        // Too many floaters at once would just be noise in fast games.
        let Some((holder, _)) = boxes.iter().find(|(_, slot)| slot.0 == score.0) else {
            continue;
        };
        commands.spawn((
            Floater {
                age: 0.0,
                colour: colour(score.0),
            },
            Text::new("+1"),
            bold(24.0),
            TextColor(colour(score.0)),
            Node {
                position_type: PositionType::Absolute,
                top: px(4),
                ..default()
            },
            UiTransform::IDENTITY,
            Pickable::IGNORE,
            ChildOf(holder),
        ));
    }
}

fn floaters(mut commands: Commands, time: Res<Time>, mut items: Query<(Entity, &mut Floater, &mut UiTransform, &mut TextColor)>) {
    for (entity, mut item, mut transform, mut ink) in &mut items {
        item.age += time.delta_secs();
        let t = (item.age / 1.1).min(1.0);
        let rise = 1.0 - (1.0 - t).powi(2);
        transform.translation = Val2::px(34.0, -6.0 - rise * 46.0);
        transform.scale = Vec2::splat(1.0 + 0.3 * (1.0 - t));
        ink.0 = item.colour.with_alpha(1.0 - t * t);
        if t >= 1.0 {
            commands.entity(entity).despawn();
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

fn pops(time: Res<Time>, mut texts: Query<(&mut Pop, &mut UiTransform, &mut TextColor)>) {
    for (mut pop, mut transform, mut ink) in &mut texts {
        if pop.age > 2.0 {
            continue;
        }
        pop.age += time.delta_secs();
        let kick = (-pop.age * 7.0).exp();
        let scale = 1.0 + pop.strength * kick * (pop.age * 16.0).cos().abs();
        transform.scale = Vec2::splat(scale);
        ink.0 = pop.base.mix(&ACCENT, kick.min(1.0));
    }
}

/// Panels slide in each time the arena opens.
fn intro(time: Res<Time>, mut clock: ResMut<IntroClock>, mut panels: Query<(&Enter, &mut UiTransform)>) {
    if clock.0 > 3.0 {
        return;
    }
    clock.0 += time.delta_secs();
    for (enter, mut transform) in &mut panels {
        let t = ((clock.0 - enter.delay) / 0.7).clamp(0.0, 1.0);
        let rest = 1.0 - ease_out_back(t);
        transform.translation = Val2::px(enter.from.x * rest, enter.from.y * rest);
    }
}

/// The banner under the score: grows in and glows orange while it shows a result.
fn banner_glow(time: Res<Time>, readout: Res<Readout>, mut banners: Query<(&mut Banner, &mut Text, &mut TextColor, &mut UiTransform)>) {
    for (mut banner, mut text, mut ink, mut transform) in &mut banners {
        set(&mut text, &readout.banner);
        if banner.result != readout.banner_result {
            banner.result = readout.banner_result;
            banner.age = 0.0;
        }
        banner.age += time.delta_secs();
        if banner.result {
            let t = (banner.age / 0.5).min(1.0);
            let pulse = (banner.age * 5.0).sin() * 0.5 + 0.5;
            transform.scale = Vec2::splat(0.7 + 0.35 * ease_out_back(t) - 0.05 * (1.0 - t));
            ink.0 = ACCENT.mix(&TEXT, pulse * 0.45);
        } else {
            transform.scale = Vec2::ONE;
            ink.0 = MUTED;
        }
    }
}

fn win_flash(
    time: Res<Time>,
    settings: Res<Settings>,
    mut won: MessageReader<Won>,
    mut flashes: Query<(&mut EdgeFlash, &mut BorderColor)>,
) {
    let latest = won.read().last().map(|win| win.player);
    for (mut flash, mut border) in &mut flashes {
        // Fast games win many times a second: only flash when there is time to see it.
        if let Some(player) = latest
            && settings.animated()
            && flash.age > 0.8
        {
            flash.age = 0.0;
            flash.colour = colour(player);
        }
        flash.age += time.delta_secs();
        let alpha = 0.85 * (-flash.age * 2.5).exp();
        let wanted = BorderColor::all(flash.colour.with_alpha(if alpha < 0.01 { 0.0 } else { alpha }));
        if *border != wanted {
            *border = wanted;
        }
    }
}

/// Tells the camera how much of the window the panels cover.
/// The window size the arena layout is drawn for; other windows scale it to fit.
const DESIGN_WIDTH: f32 = 1600.0;
const DESIGN_HEIGHT: f32 = 920.0;

/// Scales the whole UI with the window (maximised, fullscreen, a projector), so the
/// panels always fit: smaller on a short screen, larger on a big one.
fn fit_ui(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut scale: ResMut<UiScale>) {
    let Ok(window) = windows.single() else {
        return;
    };
    if window.width() <= 0.0 || window.height() <= 0.0 {
        return;
    }
    let wanted = (window.width() / DESIGN_WIDTH).min(window.height() / DESIGN_HEIGHT).clamp(0.5, 1.6);
    if (scale.0 - wanted).abs() > 0.005 {
        scale.0 = wanted;
    }
}

fn measure(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    scale: Res<UiScale>,
    top: Query<&ComputedNode, With<TopBar>>,
    sides: Query<&ComputedNode, With<SidePanel>>,
    bottom: Query<&ComputedNode, With<BottomBar>>,
    mut margins: ResMut<UiMargins>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    // Sizes in window (logical) pixels, whatever the UI scale.
    let logical = |node: &ComputedNode| node.size / window.scale_factor().max(0.01);
    let pad = PAD * scale.0;
    let side = sides.iter().map(|node| logical(node).x).fold(0.0, f32::max);
    let top = top.iter().map(|node| logical(node).y).fold(0.0, f32::max);
    let bottom = bottom.iter().map(|node| logical(node).y).fold(0.0, f32::max);
    if side <= 0.0 || bottom <= 0.0 || window.width() <= 0.0 {
        return;
    }
    let wanted = UiMargins {
        left: pad + side + 10.0,
        right: pad + side + 10.0,
        top: pad + top + 10.0,
        bottom: pad + bottom + 10.0,
    };
    let differs = (wanted.left - margins.left).abs() > 0.5
        || (wanted.top - margins.top).abs() > 0.5
        || (wanted.bottom - margins.bottom).abs() > 0.5;
    if differs {
        *margins = wanted;
    }
}
