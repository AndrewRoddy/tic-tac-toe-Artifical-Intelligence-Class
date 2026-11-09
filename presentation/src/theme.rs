//! Colours and fonts, modelled on the "Drewgo Ember" palette from Watch-Ember
//! (`shared/colors.ts`, dark mode): warm charcoal surfaces, cream text, one orange accent.
//! Fonts follow it too: a thin serif for the title, the system sans for everything else.

use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::text::FontSource;

const fn hex(rgb: u32) -> Color {
    hexa(rgb, 1.0)
}

const fn hexa(rgb: u32, alpha: f32) -> Color {
    // Exact for 0..=255, so the casts below lose nothing.
    #[expect(clippy::cast_precision_loss, reason = "a byte fits in f32 exactly")]
    const fn byte(value: u32) -> f32 {
        (value & 0xFF) as f32 / 255.0
    }
    Color::srgba(byte(rgb >> 16), byte(rgb >> 8), byte(rgb), alpha)
}

pub const BG: Color = hex(0x0028_2820);
/// Ember's surface colour, slightly see-through so the scene shows behind panels.
pub const PANEL: Color = hexa(0x0034_342c, 0.94);
pub const RAISED: Color = hex(0x003f_3f35);
pub const BORDER: Color = hex(0x004a_4a3e);
pub const HOVER: Color = hex(0x0050_5043);
pub const PRESS: Color = hex(0x005c_5c4d);
pub const TEXT: Color = hex(0x00eb_dbb2);
pub const MUTED: Color = hex(0x00bd_b394);
pub const FAINT: Color = hex(0x008c_856d);
pub const ACCENT: Color = hex(0x00ff_922f);
pub const ON_ACCENT: Color = hex(0x0028_2820);
/// Ember's "success", and the gruvbox aqua beside it: agent family colours.
pub const GREEN: Color = hex(0x00b8_bb26);
pub const AQUA: Color = hex(0x0083_a598);
/// Ember's "danger" and "star": the two disc colours.
pub const RED: Color = hex(0x00fb_4934);
pub const YELLOW: Color = hex(0x00fa_bd2f);
/// The board. Ember has no blue, so this is the gruvbox blue it sits beside.
pub const BOARD: Color = hex(0x002c_6a70);
/// The table under the board: a darker, browner surface.
pub const TABLE: Color = hex(0x001c_1812);

/// The 2D UI's fonts, read from the Windows font folder at startup. (Bevy's
/// `system_font_discovery` would find them by name, but in 0.20 it leaked memory on every
/// text change, hundreds of MB a second in fast games.)
static FONTS: OnceLock<(Handle<Font>, Handle<Font>, Handle<Font>)> = OnceLock::new();

fn font_file(names: &[&str]) -> Option<Vec<u8>> {
    let dir = std::env::var_os("WINDIR").map_or_else(|| "C:/Windows".into(), std::path::PathBuf::from).join("Fonts");
    names.iter().find_map(|name| std::fs::read(dir.join(name)).ok())
}

/// Loads the UI fonts; runs before the UI is built.
pub fn load_fonts(mut fonts: ResMut<Assets<Font>>) {
    let mut load = |names: &[&str]| font_file(names).map_or_else(Handle::default, |bytes| fonts.add(Font::from_bytes(bytes)));
    let ui = load(&["segoeui.ttf", "arial.ttf"]);
    let title = load(&["times.ttf", "georgia.ttf"]);
    let bold = load(&["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"]);
    FONTS.set((ui, title, bold)).ok();
}

/// Segoe UI (falls back to Bevy's built-in font).
pub fn ui_font() -> FontSource {
    FONTS.get().map_or_else(FontSource::default, |(ui, ..)| FontSource::Handle(ui.clone()))
}

/// Segoe UI Semibold, for headings and numbers.
pub fn bold_font() -> FontSource {
    FONTS.get().map_or_else(FontSource::default, |(.., bold)| FontSource::Handle(bold.clone()))
}

/// The serif used for Watch-Ember's logo.
pub fn title_font() -> FontSource {
    FONTS.get().map_or_else(FontSource::default, |(_, title, _)| FontSource::Handle(title.clone()))
}
