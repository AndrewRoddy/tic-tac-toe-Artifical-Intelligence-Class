//! Screen UI: agent pickers and cards on the sides, controls and status at the bottom.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button};

use crate::game::{Action, Pending, Player, Settings, UiStyle};
use crate::readout::{Readout, ReadoutSet};
use crate::present::Mode;
use crate::roster::ROSTER;
use crate::theme::{ACCENT, BORDER, FAINT, HOVER, MUTED, ON_ACCENT, PANEL, PRESS, RAISED, RED, TEXT, YELLOW, bold_font, title_font, ui_font};

const BUTTON: Color = RAISED;
const SIDE_WIDTH: f32 = 290.0;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_ui)
            .add_observer(on_activate)
            .add_systems(
                Update,
                (style_buttons, update_labels, update_cards, update_status, update_score)
                    .after(ReadoutSet)
                    // Only while the flat UI is on screen: re-laying out hidden text every
                    // move leaked memory.
                    .run_if(|style: Res<UiStyle>| !style.solid),
            )
            .add_systems(Update, show_ui);
    }
}

/// The whole 2D overlay, hidden while presenting (the slides are all 3D).
#[derive(Component)]
struct UiRoot;

/// The flat UI shows in the arena only in backup mode (the 3D HUD is the default).
fn show_ui(style: Res<UiStyle>, mode: Res<State<Mode>>, mut roots: Query<&mut Node, With<UiRoot>>) {
    let shown = !style.solid && *mode.get() == Mode::Arena;
    let wanted = if shown { Display::Flex } else { Display::None };
    for mut node in &mut roots {
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

#[derive(Component)]
struct SizeLabel;
#[derive(Component)]
struct TimeLabel;
#[derive(Component)]
struct BoardsLabel;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct MovesText;
#[derive(Component)]
struct Banner;
#[derive(Component)]
struct ScoreText(Player);
#[derive(Component)]
struct ScoreNote;
#[derive(Component)]
struct ThinkFill;
#[derive(Component)]
struct CardTitle(Player);
#[derive(Component)]
struct CardFamily(Player);
#[derive(Component)]
struct CardHow(Player);
#[derive(Component)]
struct CardStats(Player);
#[derive(Component)]
struct EloFill(Player);

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

fn button(action: Action, text: &str, width: Val) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        action,
        Node {
            width,
            min_height: px(30),
            padding: UiRect::axes(px(10), px(5)),
            margin: UiRect::all(px(2)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BorderColor::all(BORDER),
        BackgroundColor(BUTTON),
        children![if action == Action::PlayPause {
            (Text::new(text), bold(16.0), TextColor(ON_ACCENT))
        } else {
            (Text::new(text), font(15.0), TextColor(TEXT))
        }],
    )
}

fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(12)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    }
}

fn panel(node: Node) -> impl Bundle {
    (node, BackgroundColor(PANEL), BorderColor::all(BORDER))
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
                padding: UiRect::all(px(10)),
                ..default()
            },
        ))
        .with_children(|root| {
            // Top right: start the presentation.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: px(12),
                right: px(12),
                ..default()
            })
            .with_children(|corner| {
                corner.spawn(button(Action::ToggleUi, "3D UI  (u)", px(130)));
                corner.spawn(button(Action::Present, "present  (p)", px(150)));
            });
            // Title row.
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|top| {
                top.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Baseline,
                    column_gap: px(14),
                    ..default()
                })
                .with_children(|title| {
                    title.spawn((
                        Text::new("Connect Four AI"),
                        TextFont {
                            font: title_font(),
                            font_size: FontSize::Px(34.0),
                            weight: FontWeight::THIN,
                            ..default()
                        },
                        TextColor(TEXT),
                    ));
                    title.spawn(label("train on 8x7  \u{b7}  test on 9x8", 15.0, FAINT));
                });
                top.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(14),
                    ..default()
                })
                .with_children(|score| {
                    score.spawn((Text::new("0"), bold(32.0), TextColor(RED), ScoreText(Player::Red)));
                    score.spawn(label(":", 28.0, FAINT));
                    score.spawn((Text::new("0"), bold(32.0), TextColor(YELLOW), ScoreText(Player::Yellow)));
                    score.spawn((label("", 15.0, FAINT), ScoreNote));
                });
                top.spawn((Text::new(""), bold(20.0), TextColor(ACCENT), Banner));
            });

            // Middle row: two agent panels with the 3D board between them.
            root.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexStart,
                margin: UiRect::vertical(px(8)),
                ..default()
            })
            .with_children(|middle| {
                for player in [Player::Red, Player::Yellow] {
                    agent_panel(middle, player);
                }
            });

            // Bottom bar.
            root.spawn(panel(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..panel_node()
            }))
            .with_children(controls);
        });
}

fn agent_panel(parent: &mut ChildSpawnerCommands<'_>, player: Player) {
    let heading = match player {
        Player::Red => "red  \u{b7}  moves first",
        Player::Yellow => "yellow  \u{b7}  moves second",
    };
    parent
        .spawn(panel(Node {
            width: px(SIDE_WIDTH),
            row_gap: px(2),
            ..panel_node()
        }))
        .with_children(|panel| {
            panel.spawn((Text::new(heading), bold(17.0), TextColor(colour(player))));
            panel.spawn(Node { height: px(6), ..default() });
            for (agent, info) in ROSTER.iter().enumerate() {
                panel.spawn(button(Action::Pick(player, agent), info.title, percent(100)));
            }
            panel.spawn(Node { height: px(10), ..default() });
            panel.spawn((Text::new(""), bold(18.0), TextColor(TEXT), CardTitle(player)));
            panel.spawn((label("", 13.0, FAINT), CardFamily(player)));
            panel.spawn((label("", 14.0, MUTED), CardHow(player), Node { margin: UiRect::vertical(px(6)), ..default() }));
            panel.spawn((label("", 14.0, TEXT), CardStats(player)));
            panel
                .spawn((
                    Node {
                        width: percent(100),
                        height: px(8),
                        margin: UiRect::top(px(4)),
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
                        EloFill(player),
                    ));
                });
        });
}

fn controls(bar: &mut ChildSpawnerCommands<'_>) {
    let row = || Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(6),
        ..default()
    };
    bar.spawn(row()).with_children(|line| {
        line.spawn(label("board", 15.0, FAINT));
        line.spawn(button(Action::Shrink, "-", px(36)));
        line.spawn((label("", 18.0, TEXT), SizeLabel, Node { width: px(200), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }));
        line.spawn(button(Action::Grow, "+", px(36)));
        line.spawn(Node { width: px(18), ..default() });
        line.spawn(label("time", 15.0, FAINT));
        line.spawn(button(Action::Faster, "-", px(36)));
        line.spawn((label("", 18.0, TEXT), TimeLabel, Node { width: px(150), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }));
        line.spawn(button(Action::Slower, "+", px(36)));
        line.spawn(Node { width: px(18), ..default() });
        line.spawn(label("opening", 15.0, FAINT));
        line.spawn(button(Action::CycleOpening, "", px(170)));
        line.spawn(Node { width: px(18), ..default() });
        line.spawn(label("boards", 15.0, FAINT));
        line.spawn(button(Action::FewerBoards, "-", px(36)));
        line.spawn((label("", 18.0, TEXT), BoardsLabel, Node { width: px(26), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }));
        line.spawn(button(Action::MoreBoards, "+", px(36)));
        line.spawn(Node { flex_grow: 1.0, ..default() });
        line.spawn(button(Action::PlayPause, "", px(140)));
        line.spawn(button(Action::Step, "Step", px(80)));
        line.spawn(button(Action::Reset, "Reset", px(80)));
    });
    bar.spawn(row()).with_children(|line| {
        line.spawn((label("", 18.0, TEXT), StatusText, Node { flex_grow: 1.0, ..default() }));
        line.spawn((
            Node {
                width: px(260),
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
    bar.spawn((label("", 14.0, MUTED), MovesText));
    bar.spawn(label(
        "p present  \u{b7}  space play/pause  \u{b7}  , . boards  \u{b7}  s step  \u{b7}  r reset  \u{b7}  +/- board size  \u{b7}  [ ] time  \u{b7}  o opening  \u{b7}  right-drag orbit, wheel zoom",
        12.0,
        FAINT,
    ));
}

fn on_activate(event: On<Activate>, actions: Query<&Action>, mut pending: ResMut<Pending>) {
    if let Ok(&action) = actions.get(event.entity) {
        pending.0.push(action);
    }
}

fn style_buttons(
    settings: Res<Settings>,
    mut buttons: Query<(&Action, &Hovered, Has<Pressed>, Has<InteractionDisabled>, &mut BackgroundColor, &mut BorderColor)>,
) {
    for (action, hovered, pressed, disabled, mut background, mut border) in &mut buttons {
        let selected = match *action {
            Action::Pick(player, agent) => (settings.agent[player.seat()] == agent).then_some(colour(player)),
            _ => None,
        };
        // Play is Ember's filled orange primary button; it fades a little on hover.
        let base = match (*action == Action::PlayPause, pressed, hovered.get() && !disabled) {
            (true, true, _) => ACCENT.mix(&PRESS, 0.25),
            (true, false, true) => ACCENT.with_alpha(0.85),
            (true, false, false) => ACCENT,
            (false, true, _) => PRESS,
            (false, false, true) => HOVER,
            (false, false, false) => BUTTON,
        };
        let (fill, edge) = match selected {
            Some(tint) => (tint.mix(&base, 0.65), tint),
            None if *action == Action::PlayPause => (base, ACCENT),
            None => (base, BORDER),
        };
        let alpha = if disabled && selected.is_none() { 0.45 } else { 1.0 };
        *background = BackgroundColor(fill.with_alpha(alpha));
        *border = BorderColor::all(edge.with_alpha(alpha));
    }
}

fn set(text: &mut Text, value: &str) {
    if text.0 != value {
        value.clone_into(&mut text.0);
    }
}

/// Seconds since a score last went up, for its bump.
#[derive(Component, Default)]
struct Bump(f32);

fn update_score(
    mut commands: Commands,
    time: Res<Time>,
    readout: Res<Readout>,
    mut texts: ParamSet<(
        Query<(Entity, &ScoreText, &mut Text, Option<&mut Bump>)>,
        Query<&mut Text, With<ScoreNote>>,
    )>,
) {
    for (entity, score, mut text, bump) in &mut texts.p0() {
        let value = readout.score[score.0.seat()].to_string();
        let changed = text.0 != value;
        set(&mut text, &value);
        // The number pops up and settles back each time that side scores.
        let age = match bump {
            Some(mut bump) => {
                bump.0 = if changed { 0.0 } else { bump.0 + time.delta_secs() };
                bump.0
            }
            None => {
                commands.entity(entity).insert(Bump(1.0));
                1.0
            }
        };
        let scale = 1.0 + 0.55 * (-age * 7.0).exp() * (age * 18.0).cos().abs();
        commands.entity(entity).insert(UiTransform {
            scale: Vec2::splat(scale),
            ..UiTransform::IDENTITY
        });
    }
    let note = format!("   {}", readout.note);
    for mut text in &mut texts.p1() {
        set(&mut text, &note);
    }
}

fn update_labels(
    readout: Res<Readout>,
    mut texts: ParamSet<(
        Query<&mut Text, With<SizeLabel>>,
        Query<&mut Text, With<TimeLabel>>,
        Query<(&Action, &Children)>,
        Query<&mut Text>,
        Query<&mut Text, With<BoardsLabel>>,
    )>,
) {
    if !readout.is_changed() {
        return;
    }
    for mut text in &mut texts.p4() {
        set(&mut text, &readout.boards);
    }
    for mut text in &mut texts.p0() {
        set(&mut text, &readout.size);
    }
    for mut text in &mut texts.p1() {
        set(&mut text, &readout.time);
    }
    let wanted: Vec<(Entity, String)> = texts
        .p2()
        .iter()
        .filter_map(|(action, children)| {
            let value = match action {
                Action::PlayPause => readout.play.clone(),
                Action::CycleOpening => readout.opening.clone(),
                _ => return None,
            };
            children.first().map(|&child| (child, value))
        })
        .collect();
    let mut all = texts.p3();
    for (entity, value) in wanted {
        if let Ok(mut text) = all.get_mut(entity) {
            set(&mut text, &value);
        }
    }
}

fn update_cards(
    readout: Res<Readout>,
    mut texts: ParamSet<(
        Query<(&CardTitle, &mut Text)>,
        Query<(&CardFamily, &mut Text)>,
        Query<(&CardHow, &mut Text)>,
        Query<(&CardStats, &mut Text)>,
    )>,
    mut bars: Query<(&EloFill, &mut Node)>,
) {
    if !readout.is_changed() {
        return;
    }
    let card = |player: Player| &readout.cards[player.seat()];
    for (slot, mut text) in &mut texts.p0() {
        set(&mut text, &card(slot.0).title);
    }
    for (slot, mut text) in &mut texts.p1() {
        set(&mut text, &card(slot.0).family);
    }
    for (slot, mut text) in &mut texts.p2() {
        set(&mut text, &card(slot.0).how);
    }
    for (slot, mut text) in &mut texts.p3() {
        set(&mut text, &card(slot.0).stats.join("\n"));
    }
    for (bar, mut node) in &mut bars {
        node.width = percent(card(bar.0).elo_share * 100.0);
    }
}

fn update_status(
    readout: Res<Readout>,
    mut texts: ParamSet<(Query<&mut Text, With<StatusText>>, Query<&mut Text, With<MovesText>>, Query<&mut Text, With<Banner>>)>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<ThinkFill>>,
) {
    for (mut node, mut background) in &mut fill {
        node.width = percent(readout.think * 100.0);
        background.0 = colour(readout.to_move);
    }
    if !readout.is_changed() {
        return;
    }
    for mut text in &mut texts.p0() {
        set(&mut text, &readout.status);
    }
    for mut text in &mut texts.p1() {
        set(&mut text, &readout.moves);
    }
    for mut text in &mut texts.p2() {
        set(&mut text, &readout.banner);
    }
}
