//! Memo type: the editor's font, with Chinese in its paired face, and the system
//! faces the rest of the page uses.

use fumi_editor::display::Composite;
use fumi_ui::fonts::family_font;
use fumi_ui::settings;
use gpui::*;

pub fn ui(weight: FontWeight) -> Font {
    let mut f = family_font("System");
    f.weight = weight;
    f
}

pub fn serif(weight: FontWeight, italic: bool) -> Font {
    let mut f = family_font("New York");
    f.weight = weight;
    if italic {
        f.style = FontStyle::Italic;
    }
    f
}

pub fn mono() -> Font {
    family_font("SF Mono")
}

/// Monospaced digits, on any font.
pub fn tabular(mut f: Font) -> Font {
    f.features = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
    f
}

/// The natural line box of SwiftUI's text at the sizes this page uses.
pub fn line_box(size: f32) -> Pixels {
    px(match size as i32 {
        ..=12 => 15.,
        13 => 16.,
        14 | 15 => 18.,
        _ => (size * 1.2).round(),
    })
}

/// The editor's font, as Settings has it.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoType {
    pub family: String,
    pub composite: Option<Composite>,
}

impl MemoType {
    pub fn new(family: &str) -> Self {
        MemoType { family: family.to_string(), composite: Composite::for_family(family) }
    }

    pub fn from_settings() -> Self {
        Self::new(&settings::editor_font())
    }

    pub fn font(&self, bold: bool, italic: bool) -> Font {
        let mut f = family_font(&self.family);
        if bold {
            f.weight = FontWeight::BOLD;
        }
        if italic {
            f.style = FontStyle::Italic;
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_system_faces_pair_only_when_they_need_to() {
        assert_eq!(MemoType::new("System").composite, None);
        assert_eq!(MemoType::new("New York").composite.unwrap().family, "Songti SC");
        assert_eq!(MemoType::new("Avenir").composite.unwrap().family, "PingFang SC");
    }

    #[core::prelude::v1::test]
    fn line_boxes_match_appkit() {
        assert_eq!(line_box(12.), px(15.));
        assert_eq!(line_box(13.), px(16.));
        assert_eq!(line_box(15.), px(18.));
        assert_eq!(line_box(20.), px(24.));
    }
}
