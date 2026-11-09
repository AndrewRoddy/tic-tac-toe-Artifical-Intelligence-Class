//! Connect Four Arena 3D: a presentation app for the c4 project.
//! See SPEC.md. Run with `cargo run --release`.

mod engine;
mod game;
mod roster;
mod scene;
mod sizes;
mod ui;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;

use game::{Action, GamePlugin, Pending};
use scene::ScenePlugin;
use ui::UiPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Connect Four AI Arena".to_owned(),
            resolution: WindowResolution::new(1600, 900),
            ..default()
        }),
        ..default()
    }))
    .add_plugins((GamePlugin, ScenePlugin, UiPlugin));
    if let Some(capture) = Capture::from_env() {
        app.insert_resource(capture).add_systems(Update, capture_and_exit);
    }
    app.run();
}

/// Developer aid: `C4_SCREENSHOT=out.png [C4_KEYS=--[[] [C4_AUTOPLAY=1] [C4_SECONDS=6]`
/// presses the shortcut keys in `C4_KEYS` (- + [ ] o), optionally starts a match, saves a
/// screenshot after a few seconds and exits. Used to check the scene layout.
#[derive(Resource)]
struct Capture {
    path: String,
    keys: String,
    autoplay: bool,
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
            seconds: std::env::var("C4_SECONDS").ok().and_then(|text| text.parse().ok()).unwrap_or(4.0),
            stage: 0,
        })
    }
}

fn capture_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    mut capture: ResMut<Capture>,
    mut pending: ResMut<Pending>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    match capture.stage {
        0 => {
            for key in capture.keys.chars() {
                pending.0.extend(match key {
                    '-' => Some(Action::Shrink),
                    '+' => Some(Action::Grow),
                    '[' => Some(Action::Faster),
                    ']' => Some(Action::Slower),
                    'o' => Some(Action::CycleOpening),
                    _ => None,
                });
            }
            if capture.autoplay {
                pending.0.push(Action::PlayPause);
            }
            capture.stage = 1;
        }
        1 if now >= capture.seconds => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(capture.path.clone()));
            capture.stage = 2;
        }
        2 if now >= capture.seconds + 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
