//! Extruded 3D text for the presentation: real meshes built from the glyph outlines
//! of the Windows fonts (Segoe UI and Times New Roman, like the 2D UI).
//!
//! `meshtext` turns a line of text into triangles, one em high, starting at x = 0 on
//! the baseline, with depth -0.5..0.5. Meshes are cached per (face, text).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use meshtext::{Face as FontFace, MeshGenerator, MeshText, QualitySettings, TextSection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// The title serif (Times New Roman).
    Serif,
    /// Body text (Segoe UI).
    Sans,
    /// Headings and numbers (Segoe UI Semibold).
    Bold,
}

impl Face {
    const ALL: [Self; 3] = [Self::Serif, Self::Sans, Self::Bold];

    const fn files(self) -> &'static [&'static str] {
        match self {
            Self::Serif => &["times.ttf", "georgia.ttf", "segoeui.ttf", "arial.ttf"],
            Self::Sans => &["segoeui.ttf", "arial.ttf", "tahoma.ttf"],
            Self::Bold => &["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf", "segoeui.ttf", "arial.ttf"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    const fn share(self) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
}

/// A finished line of text: the mesh and its width in ems.
#[derive(Clone)]
pub struct Line {
    pub mesh: Handle<Mesh>,
    pub width: f32,
}

/// Most text meshes kept for reuse.
const CACHE_LIMIT: usize = 600;

#[derive(Resource)]
pub struct Fonts {
    generators: HashMap<Face, MeshGenerator<FontFace<'static>>>,
    cache: HashMap<(Face, bool, String), Line>,
}

fn font_dir() -> std::path::PathBuf {
    std::env::var_os("WINDIR").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from).join("Fonts")
}

/// Reads the first font file that exists and parses. The bytes live for the whole run.
fn load(face: Face) -> Option<MeshGenerator<FontFace<'static>>> {
    let dir = font_dir();
    face.files().iter().find_map(|file| {
        let bytes = std::fs::read(dir.join(file)).ok()?;
        FontFace::parse(&bytes, 0).ok()?;
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let quality = QualitySettings {
            quad_interpolation_steps: 4,
            cubic_interpolation_steps: 3,
        };
        Some(MeshGenerator::new_with_quality(leaked, quality))
    })
}

impl Default for Fonts {
    fn default() -> Self {
        let generators = Face::ALL.into_iter().filter_map(|face| load(face).map(|generator| (face, generator))).collect();
        Self {
            generators,
            cache: HashMap::new(),
        }
    }
}

impl Fonts {
    /// The mesh for one line of text, or `None` if no font could be loaded.
    /// `flat` makes just the front face (about a third of the triangles): used for small
    /// text, where the extrusion cannot be seen anyway.
    pub fn line_with(&mut self, meshes: &mut Assets<Mesh>, face: Face, text: &str, flat: bool) -> Option<Line> {
        let key = (face, flat, text.to_owned());
        if let Some(line) = self.cache.get(&key) {
            return Some(line.clone());
        }
        let generator = self.generators.get_mut(&face)?;
        let built: MeshText = generator.generate_section(text, flat, None).ok()?;
        let positions: Vec<[f32; 3]> = built.vertices.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect();
        let width = built.bbox.max.x.max(0.0);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.compute_flat_normals();
        let line = Line {
            mesh: meshes.add(mesh),
            width,
        };
        // Changing text (scores, move lists) makes new strings all the time: keep the
        // cache bounded. Meshes still on screen stay alive through their entities.
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
        }
        self.cache.insert(key, line.clone());
        Some(line)
    }
}

/// What to spawn for a line of 3D text: `size` is the em height in world units and
/// `depth` the extrusion. The text is placed so `at` is its baseline anchor.
pub struct Spec<'a> {
    pub text: &'a str,
    pub face: Face,
    pub size: f32,
    pub depth: f32,
    pub align: Align,
    /// Shrinks the text to fit this width (world units), if set.
    pub max_width: Option<f32>,
    /// Front face only (small HUD text).
    pub flat: bool,
}

impl<'a> Spec<'a> {
    pub const fn new(text: &'a str, face: Face, size: f32) -> Self {
        Self {
            text,
            face,
            size,
            // Thin for small text so it stays crisp; titles set their own depth.
            depth: size * 0.07,
            align: Align::Left,
            max_width: None,
            flat: false,
        }
    }

    pub const fn flat(mut self, flat: bool) -> Self {
        self.flat = flat;
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub const fn fit(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub const fn depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
    }
}

/// The mesh bundle for a line of text, positioned relative to its parent, or `None`
/// if no font is available.
pub fn text_bundle(
    fonts: &mut Fonts,
    meshes: &mut Assets<Mesh>,
    spec: &Spec<'_>,
    material: Handle<StandardMaterial>,
    at: Vec3,
) -> Option<impl Bundle> {
    let line = fonts.line_with(meshes, spec.face, spec.text, spec.flat)?;
    let natural = line.width * spec.size;
    let size = match spec.max_width {
        Some(max) if natural > max && natural > 0.0 => spec.size * max / natural,
        _ => spec.size,
    };
    let offset = -line.width * size * spec.align.share();
    Some((
        Mesh3d(line.mesh),
        MeshMaterial3d(material),
        Transform::from_translation(at + Vec3::new(offset, 0.0, 0.0)).with_scale(Vec3::new(size, size, spec.depth)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_fonts_make_meshes() {
        let mut fonts = Fonts::default();
        eprintln!("faces loaded: {:?}", fonts.generators.keys().collect::<Vec<_>>());
        for face in Face::ALL {
            let generator = fonts.generators.get_mut(&face).expect("font loads");
            let built: MeshText = generator.generate_section("Connect", false, None).expect("meshes");
            eprintln!("{face:?}: {} verts, bbox {:?}", built.vertices.len() / 3, built.bbox);
        }
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    #[test]
    fn long_line_cost() {
        let mut fonts = Fonts::default();
        let mut meshes = Assets::<Mesh>::default();
        let started = std::time::Instant::now();
        for n in 0..20 {
            let text = format!("Red (MCTS (UCT)) is thinking...   (last: Yellow played column {n} in 1000 ms)");
            fonts.line_with(&mut meshes, Face::Sans, &text, false);
        }
        eprintln!("20 status lines: {:?}", started.elapsed());
    }
}
