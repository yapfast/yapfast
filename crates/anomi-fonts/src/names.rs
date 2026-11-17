//! The eleven faces Discord's client draws display name styles in, from
//! `ui/fonts` (see its README for where each comes from and its licence). A
//! style names its face by a number, Discord's `font_id`; [`Name::of`] reads
//! it. Each face has one weight, which the name is drawn in whatever weight
//! the row asks for: `anomi_theme::fonts` registers it at every weight.
//!
//! Ports `ui/fonts/README.md`'s display name fonts and `family`, `weight` of
//! `src/ui/name_style.rs`.

/// A display name face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Name {
    /// Cherry Bomb One.
    Sakura,
    /// Chicle.
    Jellybean,
    /// MuseoModerno.
    Modern,
    /// Max Lillo's Néo-castel.
    Medieval,
    /// Pixelify Sans.
    EightBit,
    /// Sinistre, by Jules Durand and Collletttivo.
    Vampyre,
    /// Zilla Slab.
    Tempo,
    /// Playpen Sans.
    MonkeyBars,
    /// Orbitron.
    Mainframe,
    /// New Rocker.
    Headbang,
    /// Kalam.
    Journal,
}

impl Name {
    /// Every face, in the order of Discord's numbers.
    pub const ALL: [Name; 11] = [
        Name::Sakura,
        Name::Jellybean,
        Name::Modern,
        Name::Medieval,
        Name::EightBit,
        Name::Vampyre,
        Name::Tempo,
        Name::MonkeyBars,
        Name::Mainframe,
        Name::Headbang,
        Name::Journal,
    ];

    /// The face of a style's `font_id`, or none for 11 and the retired ids,
    /// which draw in the usual font.
    pub const fn of(font_id: u8) -> Option<Self> {
        Some(match font_id {
            3 => Self::Sakura,
            4 => Self::Jellybean,
            6 => Self::Modern,
            7 => Self::Medieval,
            8 => Self::EightBit,
            10 => Self::Vampyre,
            12 => Self::Tempo,
            13 => Self::MonkeyBars,
            14 => Self::Mainframe,
            15 => Self::Headbang,
            16 => Self::Journal,
            _ => return None,
        })
    }

    /// The family the face's file names, which text asks for. Medieval and
    /// Vampyre keep their designers' family names.
    pub const fn family(self) -> &'static str {
        match self {
            Self::Sakura => "Sakura",
            Self::Jellybean => "Jellybean",
            Self::Modern => "Modern",
            Self::Medieval => "Néo-castel",
            Self::EightBit => "8Bit",
            Self::Vampyre => "Sinistre",
            Self::Tempo => "Tempo",
            Self::MonkeyBars => "Monkey Bars",
            Self::Mainframe => "Mainframe",
            Self::Headbang => "Headbang",
            Self::Journal => "Journal",
        }
    }

    /// The weight of its file's one face, as the file declares it.
    pub const fn weight(self) -> u16 {
        match self {
            Self::Modern | Self::EightBit => 500,
            Self::Vampyre => 548,
            Self::Tempo => 600,
            Self::MonkeyBars | Self::Mainframe | Self::Journal => 700,
            _ => 400,
        }
    }

    /// The face's file.
    pub fn data(self) -> &'static [u8] {
        match self {
            Self::Sakura => SAKURA,
            Self::Jellybean => JELLYBEAN,
            Self::Modern => MODERN,
            Self::Medieval => MEDIEVAL,
            Self::EightBit => EIGHT_BIT,
            Self::Vampyre => VAMPYRE,
            Self::Tempo => TEMPO,
            Self::MonkeyBars => MONKEY_BARS,
            Self::Mainframe => MAINFRAME,
            Self::Headbang => HEADBANG,
            Self::Journal => JOURNAL,
        }
    }
}

macro_rules! faces {
    ($($name:ident = $file:literal;)*) => {
        $(
            #[doc = concat!("`ui/fonts/", $file, "`.")]
            pub static $name: &[u8] = include_bytes!(concat!("../../../ui/fonts/", $file));
        )*
    };
}

faces! {
    SAKURA = "DisplayName-Sakura.ttf";
    JELLYBEAN = "DisplayName-Jellybean.ttf";
    MODERN = "DisplayName-Modern.ttf";
    MEDIEVAL = "DisplayName-Medieval.otf";
    EIGHT_BIT = "DisplayName-8Bit.ttf";
    VAMPYRE = "DisplayName-Vampyre.ttf";
    TEMPO = "DisplayName-Tempo.ttf";
    MONKEY_BARS = "DisplayName-MonkeyBars.ttf";
    MAINFRAME = "DisplayName-Mainframe.ttf";
    HEADBANG = "DisplayName-Headbang.ttf";
    JOURNAL = "DisplayName-Journal.ttf";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discords_numbers_name_the_faces() {
        assert_eq!(Name::of(14).map(Name::family), Some("Mainframe"));
        assert_eq!(Name::of(11), None);
        assert_eq!(Name::of(7).map(Name::family), Some("Néo-castel"));
        assert_eq!(
            Name::of(10).map(|face| (face.family(), face.weight())),
            Some(("Sinistre", 548))
        );
        let numbered: Vec<_> = (0..=20).filter_map(Name::of).collect();
        assert_eq!(numbered, Name::ALL);
    }

    #[test]
    fn each_face_declares_the_weight_given() {
        // OS/2's usWeightClass, at offset 4 of its table.
        for face in Name::ALL {
            let data = face.data();
            let count = usize::from(u16::from_be_bytes([data[4], data[5]]));
            let os2 = (0..count)
                .map(|index| &data[12 + 16 * index..28 + 16 * index])
                .find(|record| &record[..4] == b"OS/2")
                .expect("an OS/2 table");
            let at = u32::from_be_bytes(os2[8..12].try_into().unwrap()) as usize;
            let weight = u16::from_be_bytes([data[at + 4], data[at + 5]]);
            assert_eq!(weight, face.weight(), "{face:?}");
        }
    }
}
