//! Screen UI: agent pickers and cards on the sides, controls and status at the bottom.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button};

use crate::game::{Action, GameFlow, Game, OPENINGS, Outcome, Pending, Phase, Player, Series, Settings};
use crate::roster::ROSTER;
use crate::scene::{RED, YELLOW};
use crate::sizes::LADDER;

const PANEL: Color = Color::srgba(0.05, 0.06, 0.1, 0.88);
const BUTTON: Color = Color::srgb(0.13, 0.15, 0.21);
const BUTTON_HOVER: Color = Color::srgb(0.2, 0.23, 0.32);
const BUTTON_PRESS: Color = Color::srgb(0.28, 0.32, 0.45);
const BORDER: Color = Color::srgb(0.22, 0.25, 0.34);
const TEXT: Color = Color::srgb(0.92, 0.93, 0.96);
const MUTED: Color = Color::srgb(0.6, 0.64, 0.72);
const ACCENT: Color = Color::srgb(0.35, 0.6, 1.0);
const SIDE_WIDTH: f32 = 290.0;
const TOP_ELO: f32 = 740.0;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_ui)
            .add_observer(on_activate)
            .add_systems(Update, (style_buttons, update_labels, update_cards, update_status, update_score).after(GameFlow));
    }
}

#[derive(Component)]
struct SizeLabel;
#[derive(Component)]
struct TimeLabel;
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
        font_size: FontSize::Px(size),
        ..default()
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
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BorderColor::all(BORDER),
        BackgroundColor(BUTTON),
        children![label(text, 16.0, TEXT)],
    )
}

fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(12)),
        border_radius: BorderRadius::all(px(12)),
        ..default()
    }
}

fn build_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::all(px(10)),
            ..default()
        })
        .with_children(|root| {
            // Title row.
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|top| {
                top.spawn(label("Connect Four AI: Train on 8x7, Test on 9x8", 26.0, TEXT));
                top.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(14),
                    ..default()
                })
                .with_children(|score| {
                    score.spawn((label("0", 34.0, RED), ScoreText(Player::Red)));
                    score.spawn(label(":", 30.0, MUTED));
                    score.spawn((label("0", 34.0, YELLOW), ScoreText(Player::Yellow)));
                    score.spawn((label("", 16.0, MUTED), ScoreNote));
                });
                top.spawn((label("", 22.0, ACCENT), Banner));
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
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..panel_node()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(controls);
        });
}

fn agent_panel(parent: &mut ChildSpawnerCommands<'_>, player: Player) {
    let heading = match player {
        Player::Red => "RED - moves first",
        Player::Yellow => "YELLOW - moves second",
    };
    parent
        .spawn((
            Node {
                width: px(SIDE_WIDTH),
                row_gap: px(2),
                ..panel_node()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|panel| {
            panel.spawn(label(heading, 18.0, colour(player)));
            panel.spawn(Node { height: px(6), ..default() });
            for (agent, info) in ROSTER.iter().enumerate() {
                panel.spawn(button(Action::Pick(player, agent), info.title, percent(100)));
            }
            panel.spawn(Node { height: px(10), ..default() });
            panel.spawn((label("", 18.0, TEXT), CardTitle(player)));
            panel.spawn((label("", 13.0, MUTED), CardFamily(player)));
            panel.spawn((label("", 14.0, TEXT), CardHow(player), Node { margin: UiRect::vertical(px(6)), ..default() }));
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
        line.spawn(label("Board", 16.0, MUTED));
        line.spawn(button(Action::Shrink, "-", px(36)));
        line.spawn((label("", 18.0, TEXT), SizeLabel, Node { width: px(230), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }));
        line.spawn(button(Action::Grow, "+", px(36)));
        line.spawn(Node { width: px(18), ..default() });
        line.spawn(label("Time", 16.0, MUTED));
        line.spawn(button(Action::Faster, "-", px(36)));
        line.spawn((label("", 18.0, TEXT), TimeLabel, Node { width: px(150), flex_shrink: 0.0, justify_content: JustifyContent::Center, ..default() }));
        line.spawn(button(Action::Slower, "+", px(36)));
        line.spawn(Node { width: px(18), ..default() });
        line.spawn(label("Opening", 16.0, MUTED));
        line.spawn(button(Action::CycleOpening, "", px(170)));
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
        "Space play/pause  -  S step  -  R reset  -  +/- board size  -  [ ] time  -  O opening  -  right-drag orbit, wheel zoom",
        12.0,
        MUTED,
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
        let base = match (pressed, hovered.get() && !disabled) {
            (true, _) => BUTTON_PRESS,
            (false, true) => BUTTON_HOVER,
            (false, false) => BUTTON,
        };
        let (fill, edge) = match selected {
            Some(tint) => (tint.mix(&base, 0.65), tint),
            None => (base, BORDER),
        };
        let alpha = if disabled && selected.is_none() { 0.45 } else { 1.0 };
        *background = BackgroundColor(fill.with_alpha(alpha));
        *border = BorderColor::all(edge.with_alpha(alpha));
    }
}

fn set(text: &mut Text, value: String) {
    if text.0 != value {
        text.0 = value;
    }
}

fn time_label(seconds: f32) -> String {
    if seconds <= 0.001 {
        "1 ms / move".to_owned()
    } else if seconds < 0.1 {
        format!("{:.0} ms / move", seconds * 1000.0)
    } else {
        format!("{seconds:.2} s / move")
    }
}

fn update_score(
    series: Res<Series>,
    mut texts: ParamSet<(Query<(&ScoreText, &mut Text)>, Query<&mut Text, With<ScoreNote>>)>,
) {
    for (score, mut text) in &mut texts.p0() {
        set(&mut text, series.wins[score.0.seat()].to_string());
    }
    let games = series.games();
    let note = format!("   draws {}   -   games {}", series.draws, games);
    for mut text in &mut texts.p1() {
        set(&mut text, note.clone());
    }
}

fn update_labels(
    settings: Res<Settings>,
    series: Res<Series>,
    game: Res<Game>,
    mut texts: ParamSet<(
        Query<&mut Text, With<SizeLabel>>,
        Query<&mut Text, With<TimeLabel>>,
        Query<(&Action, &Children)>,
        Query<&mut Text>,
    )>,
) {
    let size = LADDER[settings.size];
    for mut text in &mut texts.p0() {
        set(&mut text, format!("{} x {}  {}", size.columns, size.rows, size.label));
    }
    for mut text in &mut texts.p1() {
        set(&mut text, time_label(settings.per_move()));
    }
    let play = match game.phase {
        Phase::Idle => "Play",
        Phase::Finished | Phase::Starting if series.playing => "Pause",
        Phase::Finished => "New match",
        Phase::Running => "Pause",
        Phase::Paused | Phase::Starting => "Resume",
    };
    let opening = match OPENINGS[settings.opening] {
        0 => "empty board".to_owned(),
        stones => format!("random {stones} stones"),
    };
    let wanted: Vec<(Entity, String)> = texts
        .p2()
        .iter()
        .filter_map(|(action, children)| {
            let value = match action {
                Action::PlayPause => play.to_owned(),
                Action::CycleOpening => opening.clone(),
                _ => return None,
            };
            children.first().map(|&child| (child, value))
        })
        .collect();
    let mut all = texts.p3();
    for (entity, value) in wanted {
        if let Ok(mut text) = all.get_mut(entity) {
            set(&mut text, value);
        }
    }
}

fn update_cards(
    settings: Res<Settings>,
    mut texts: ParamSet<(
        Query<(&CardTitle, &mut Text)>,
        Query<(&CardFamily, &mut Text)>,
        Query<(&CardHow, &mut Text)>,
        Query<(&CardStats, &mut Text)>,
    )>,
    mut bars: Query<(&EloFill, &mut Node)>,
) {
    let info = |player: Player| &ROSTER[settings.agent[player.seat()]];
    for (card, mut text) in &mut texts.p0() {
        set(&mut text, info(card.0).title.to_owned());
    }
    for (card, mut text) in &mut texts.p1() {
        set(&mut text, format!("{}
id: {}", info(card.0).family, info(card.0).entrant.name()));
    }
    for (card, mut text) in &mut texts.p2() {
        set(&mut text, info(card.0).how.to_owned());
    }
    for (card, mut text) in &mut texts.p3() {
        let agent = info(card.0);
        set(
            &mut text,
            format!(
                "9x8 tournament Elo: {:.0}\n8x7 move accuracy: {:.1}%\n9x8 late-game accuracy: {:.1}%",
                agent.elo,
                agent.acc_8x7 * 100.0,
                agent.acc_9x8 * 100.0
            ),
        );
    }
    for (bar, mut node) in &mut bars {
        node.width = percent((info(bar.0).elo / TOP_ELO * 100.0).clamp(1.0, 100.0));
    }
}

fn update_status(
    time: Res<Time>,
    settings: Res<Settings>,
    series: Res<Series>,
    game: Res<Game>,
    mut texts: ParamSet<(Query<&mut Text, With<StatusText>>, Query<&mut Text, With<MovesText>>, Query<&mut Text, With<Banner>>)>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<ThinkFill>>,
) {
    let agent = |player: Player| ROSTER[settings.agent[player.seat()]].title;
    let elapsed = time.elapsed_secs() - game.think_started;
    let last = game.last.map_or_else(String::new, |(player, col, millis)| {
        format!("   (last: {} played column {} in {} ms)", player.name(), col + 1, millis)
    });
    let status = if let Some(error) = &game.error {
        format!("Stopped: {error}")
    } else {
        match game.phase {
            Phase::Idle => "Pick two agents and a board size, then press Play (Space).".to_owned(),
            Phase::Starting => "Building agents...".to_owned(),
            Phase::Running if !settings.animated() => "Racing: one match after another as fast as they can think.".to_owned(),
            Phase::Running | Phase::Paused if game.waiting => format!(
                "{} ({}) is thinking...  {:.1} / {:.2} s{last}",
                game.to_move.name(),
                agent(game.to_move),
                elapsed,
                settings.per_move()
            ),
            Phase::Running => format!("{} to move{last}", game.to_move.name()),
            Phase::Paused => format!("Paused: Space to resume, S to step{last}"),
            Phase::Finished => {
                let result = match game.outcome {
                    Some(Outcome::Winner(player)) => format!("{} ({}) wins!", player.name(), agent(player)),
                    Some(Outcome::Draw) => "Draw: the board is full.".to_owned(),
                    None => String::new(),
                };
                let next = if series.playing { "   Next match coming up..." } else { "   Space for a new match." };
                format!("{result}{next}")
            }
        }
    };
    for mut text in &mut texts.p0() {
        set(&mut text, status.clone());
    }

    let moves: Vec<String> = game.moves.iter().map(|col| (col + 1).to_string()).collect();
    let (opening, played) = moves.split_at(game.opening_len.min(moves.len()));
    let mut listing = format!("Moves ({}): ", game.moves.len());
    if !opening.is_empty() {
        listing.push_str(&format!("[opening {}]  ", opening.join(" ")));
    }
    listing.push_str(&played.join(" "));
    for mut text in &mut texts.p1() {
        set(&mut text, listing.clone());
    }

    let banner = match (game.phase, game.outcome) {
        (Phase::Finished, Some(Outcome::Winner(winner))) => {
            let loser = match winner {
                Player::Red => Player::Yellow,
                Player::Yellow => Player::Red,
            };
            format!(
                "{} WINS: {} beats {} in {} moves",
                winner.name().to_uppercase(),
                agent(winner),
                agent(loser),
                game.moves.len() - game.opening_len
            )
        }
        (Phase::Finished, Some(Outcome::Draw)) => format!("DRAW: {} vs {}", agent(Player::Red), agent(Player::Yellow)),
        _ => format!("{}  vs  {}", agent(Player::Red), agent(Player::Yellow)),
    };
    for mut text in &mut texts.p2() {
        set(&mut text, banner.clone());
    }

    for (mut node, mut background) in &mut fill {
        let share = if game.waiting { (elapsed / settings.per_move()).clamp(0.0, 1.0) } else { 0.0 };
        node.width = percent(share * 100.0);
        background.0 = colour(game.to_move);
    }
}
