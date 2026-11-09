//! Connect Four Arena 3D: a presentation app for the c4 project.
//! See SPEC.md. Run with `cargo run --release`.

mod backdrop;
mod effects;
mod engine;
mod game;
mod hud;
mod present;
mod presenter;
mod readout;
mod roster;
mod scene;
mod sizes;
mod slides;
mod text3d;
mod theme;
mod ui;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;

use game::{Action, GamePlugin, Pending, Player, Series, UiStyle};
use present::{Deck, Mode, PresentPlugin};
use scene::ScenePlugin;
use ui::UiPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Connect Four AI Arena".to_owned(),
            resolution: WindowResolution::new(1600, 900),
            // C4_NOVSYNC=1 for uncapped frames (handy when measuring).
            present_mode: if std::env::var("C4_NOVSYNC").is_ok() {
                bevy::window::PresentMode::AutoNoVsync
            } else {
                bevy::window::PresentMode::AutoVsync
            },
            ..default()
        }),
        ..default()
    }))
    .add_plugins((GamePlugin, ScenePlugin, UiPlugin, PresentPlugin, effects::EffectsPlugin, backdrop::BackdropPlugin, readout::ReadoutPlugin, hud::HudPlugin, presenter::PresenterPlugin));
    // Bevy 0.20's GPU light clustering crashed the Intel Iris Xe driver (DeviceLost)
    // when games restart many times a second. We only have a few lights, so CPU
    // clustering is free; turn the GPU path off.
    app.add_systems(Startup, |mut clusters: ResMut<bevy::light::cluster::GlobalClusterSettings>| {
        clusters.gpu_clustering = None;
    });
    app.add_systems(PreStartup, theme::load_fonts);
    // C4_FPS=1 logs the frame rate once a second.
    if std::env::var("C4_FPS").is_ok() {
        app.add_plugins((
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
            bevy::diagnostic::LogDiagnosticsPlugin {
                wait_duration: std::time::Duration::from_secs(2),
                ..default()
            },
        ));
    }
    if std::env::var("C4_FPS").is_ok() {
        app.add_systems(
            Update,
            |time: Res<Time>, meshes: Res<Assets<Mesh>>, materials: Res<Assets<StandardMaterial>>, mut last: Local<f32>| {
                if time.elapsed_secs() - *last >= 2.0 {
                    *last = time.elapsed_secs();
                    info!("assets: {} meshes, {} materials", meshes.len(), materials.len());
                }
            },
        );
    }
    // C4_LITE=1 starts in the flat backup UI with bloom and shadows off.
    if std::env::var("C4_LITE").is_ok() {
        app.insert_resource(UiStyle { solid: false });
    }
    if let Some(capture) = Capture::from_env() {
        app.insert_resource(capture).add_systems(Update, (capture_and_exit, simulate_click));
    }
    app.run();
}

/// Developer aid: `C4_SCREENSHOT=out.png [C4_KEYS=--[[] [C4_AUTOPLAY=1] [C4_SECONDS=6]`
/// picks Yellow's agent `C4_YELLOW` (roster index), presses the shortcut keys in
/// `C4_KEYS` (- + [ ] o . ,), optionally starts a match or opens
/// slide `C4_SLIDE`, saves a
/// screenshot after a few seconds and exits. Used to check the scene layout.
#[derive(Resource)]
struct Capture {
    path: String,
    keys: String,
    autoplay: bool,
    /// Open the presentation at this slide (1-based).
    slide: Option<usize>,
    /// Shoot this long after the first game ends instead of at `seconds`.
    after_win: Option<f32>,
    /// Press the keys at this time instead of on the first frame.
    keys_at: f32,
    seconds: f32,
    stage: u8,
}

impl Capture {
    fn from_env() -> Option<Self> {
        let path = std::env::var("C4_SCREENSHOT").ok()?;
        Some(Self {
            path,
            keys: std::env::var("C4_KEYS").unwrap_or_default(),
            autoplay: std::env::var("C4_AUTOPLAY").is_ok(),
            slide: std::env::var("C4_SLIDE").ok().and_then(|text| text.parse().ok()),
            after_win: std::env::var("C4_AFTER_WIN").ok().and_then(|text| text.parse().ok()),
            keys_at: std::env::var("C4_KEYS_AT").ok().and_then(|text| text.parse().ok()).unwrap_or(0.0),
            seconds: std::env::var("C4_SECONDS").ok().and_then(|text| text.parse().ok()).unwrap_or(4.0),
            stage: 0,
        })
    }
}

/// Developer aid: `C4_CLICK=x,y` (window pixels) moves the pointer there and clicks,
/// at `C4_KEYS_AT`, through the same picking input a real mouse uses.
fn simulate_click(
    time: Res<Time>,
    capture: Res<Capture>,
    windows: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut frames: Local<u32>,
    mut input: MessageWriter<bevy::picking::pointer::PointerInput>,
) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
    use bevy::picking::prelude::PointerButton;
    let Some((x, y)) = std::env::var("C4_CLICK").ok().and_then(|text| {
        let (x, y) = text.split_once(',')?;
        Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
    }) else {
        return;
    };
    if time.elapsed_secs() < capture.keys_at {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(target) = bevy::window::WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    let location = Location {
        target: NormalizedRenderTarget::Window(target),
        position: Vec2::new(x, y),
    };
    let action = match *frames {
        0 => Some(PointerAction::Move { delta: Vec2::ZERO }),
        3 => Some(PointerAction::Press(PointerButton::Primary)),
        6 => Some(PointerAction::Release(PointerButton::Primary)),
        _ => None,
    };
    if let Some(action) = action {
        input.write(PointerInput::new(PointerId::Mouse, location, action));
    }
    *frames += 1;
}

fn capture_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    mut capture: ResMut<Capture>,
    mut pending: ResMut<Pending>,
    mut deck: ResMut<Deck>,
    series: Res<Series>,
    mut mode: ResMut<NextState<Mode>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    // The slide opens on the first frame; keys (e.g. `a` back to the arena) come later.
    if let Some(slide) = capture.slide.take() {
        deck.go_to(slide.saturating_sub(1));
        mode.set(Mode::Slides);
    }
    match capture.stage {
        0 if now >= capture.keys_at => {
            for key in capture.keys.chars() {
                pending.0.extend(match key {
                    '-' => Some(Action::Shrink),
                    '+' => Some(Action::Grow),
                    '[' => Some(Action::Faster),
                    ']' => Some(Action::Slower),
                    'o' => Some(Action::CycleOpening),
                    '.' => Some(Action::MoreBoards),
                    ',' => Some(Action::FewerBoards),
                    'a' => {
                        mode.set(Mode::Arena);
                        None
                    }
                    '>' => {
                        let next = deck.index + 1;
                        deck.go_to(next);
                        None
                    }
                    _ => None,
                });
            }
            if let Some(agent) = std::env::var("C4_YELLOW").ok().and_then(|text| text.parse().ok()) {
                pending.0.push(Action::Pick(Player::Yellow, agent));
            }
            if capture.autoplay {
                pending.0.push(Action::PlayPause);
            }
            capture.stage = 1;
        }
        1 if capture.after_win.is_some() && series.games() > 0 => {
            capture.seconds = now + capture.after_win.unwrap_or(0.0);
            capture.after_win = None;
        }
        1 if capture.after_win.is_none() && now >= capture.seconds => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(capture.path.clone()));
            capture.stage = 2;
        }
        2 if now >= capture.seconds + 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
