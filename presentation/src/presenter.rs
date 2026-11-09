//! Presenter window: N during the slides opens a second window (drag it to your own
//! screen) with this slide's speaker notes, its time budget, the talk clock and what
//! comes next. The projector keeps showing only the 3D slides. N again, or closing the
//! window, puts it away.

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::{WindowRef, WindowResolution};

use crate::present::{Deck, Mode};
use crate::slides::SLIDES;
use crate::theme::{ACCENT, BG, FAINT, MUTED, RED, TEXT, ui_font};

/// The talk is planned for this long.
const TALK_SECONDS: f32 = 600.0;

#[derive(Resource, Default)]
struct Presenter {
    window: Option<Entity>,
    /// When the talk clock started (first time the slides opened).
    started: Option<f32>,
    /// When the current slide was first shown, and which slide that was.
    slide_started: f32,
    slide: usize,
}

#[derive(Component)]
enum Readout {
    Slide,
    Clock,
    SlideClock,
    Notes,
    Next,
}

pub struct PresenterPlugin;

impl Plugin for PresenterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Presenter>()
            .add_systems(OnEnter(Mode::Slides), start_clock)
            .add_systems(Update, (toggle, track_slide, update).chain().run_if(in_state(Mode::Slides)))
            .add_systems(Update, forget_closed);
    }
}

fn start_clock(time: Res<Time>, mut presenter: ResMut<Presenter>) {
    let now = time.elapsed_secs();
    presenter.started.get_or_insert(now);
}

fn text(value: &str, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: ui_font(),
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(colour),
    )
}

fn toggle(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, mut presenter: ResMut<Presenter>) {
    if !keys.just_pressed(KeyCode::KeyN) {
        return;
    }
    if let Some(window) = presenter.window.take() {
        commands.entity(window).despawn();
        return;
    }
    let window = commands
        .spawn(Window {
            title: "Presenter notes".to_owned(),
            resolution: WindowResolution::new(900, 620),
            ..default()
        })
        .id();
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(BG),
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(window)),
            // Despawned with the window.
            ChildOf(window),
        ))
        .id();
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(24)),
                row_gap: px(14),
                ..default()
            },
            UiTargetCamera(camera),
            ChildOf(window),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|top| {
                top.spawn((text("", 22.0, ACCENT), Readout::Slide));
                top.spawn((text("", 30.0, TEXT), Readout::Clock));
            });
            root.spawn((text("", 18.0, MUTED), Readout::SlideClock));
            root.spawn((text("", 24.0, TEXT), Readout::Notes, Node { flex_grow: 1.0, ..default() }));
            root.spawn((text("", 18.0, FAINT), Readout::Next));
        });
    presenter.window = Some(window);
}

/// Restarts the slide timer whenever the slide changes.
fn track_slide(time: Res<Time>, deck: Res<Deck>, mut presenter: ResMut<Presenter>) {
    if presenter.slide != deck.index {
        presenter.slide = deck.index;
        presenter.slide_started = time.elapsed_secs();
    }
}

fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.0).floor();
    let minutes = (whole / 60.0).floor();
    format!("{minutes:.0}:{:02.0}", whole - minutes * 60.0)
}

fn update(
    time: Res<Time>,
    deck: Res<Deck>,
    presenter: Res<Presenter>,
    mut texts: Query<(&Readout, &mut Text, &mut TextColor)>,
) {
    if presenter.window.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let slide = &SLIDES[deck.index];
    let on_slide = now - presenter.slide_started;
    let total = presenter.started.map_or(0.0, |started| now - started);
    // Where the talk should be by the start of this slide.
    let planned: u32 = SLIDES[..deck.index].iter().map(|slide| slide.seconds).sum();
    let budget = f32::from(u16::try_from(slide.seconds).unwrap_or(u16::MAX));
    for (readout, mut text, mut colour) in &mut texts {
        let (value, tint) = match readout {
            Readout::Slide => (format!("{} / {}   {}", deck.index + 1, SLIDES.len(), slide.title), ACCENT),
            Readout::Clock => {
                let late = total > TALK_SECONDS;
                (format!("{} / {}", clock(total), clock(TALK_SECONDS)), if late { RED } else { TEXT })
            }
            Readout::SlideClock => {
                let over = on_slide > budget;
                let ahead = f32::from(u16::try_from(planned).unwrap_or(u16::MAX)) - (total - on_slide);
                let pace = if ahead >= 0.0 { format!("{} ahead of plan", clock(ahead)) } else { format!("{} behind plan", clock(-ahead)) };
                (
                    format!("this slide {} of {}    \u{b7}    {pace}", clock(on_slide), clock(budget)),
                    if over { RED } else { MUTED },
                )
            }
            Readout::Notes => (slide.notes.to_owned(), TEXT),
            Readout::Next => (
                SLIDES.get(deck.index + 1).map_or_else(
                    || "next: the arena (Esc)".to_owned(),
                    |next| format!("next: {}", next.title),
                ),
                FAINT,
            ),
        };
        if text.0 != value {
            text.0 = value;
        }
        if colour.0 != tint {
            colour.0 = tint;
        }
    }
}

/// The window may be closed with its own close button.
fn forget_closed(mut presenter: ResMut<Presenter>, windows: Query<(), With<Window>>) {
    if let Some(window) = presenter.window
        && windows.get(window).is_err()
    {
        presenter.window = None;
    }
}
