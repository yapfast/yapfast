# Custom Font & Emoji Font Support Design Specification

## Overview

Yapfast allows users to customize their visual interface, but font choices are currently hardcoded to DM Sans for UI text and bundled Twemoji ("Yapfast Twemoji") for emojis.

This specification introduces custom font and emoji font support:
1. **Text Fonts**: Users can choose among built-in default fonts (DM Sans, Inter, System Default) or import any custom font (`.ttf`, `.otf`). Custom fonts are saved to a list and selectable at any time.
2. **Emoji Fonts**: Users can choose between default Discord emojis (Twemoji) or import alternative emoji font styles (such as JoyPixels, Apple Color Emoji, Noto Color Emoji, Fluent Emoji, etc.).
3. **Live Dynamic Reshaping**: Selected and imported fonts are applied immediately across the entire user interface and text shapers without requiring an application restart.

---

## Data Model & Persistence

### 1. File Storage
- Imported font files are copied into the client root data directory:
  `<root>/fonts/<id>.<ext>`
  where `<id>` is a unique UUID v4 string (or sanitized slug) and `<ext>` matches the original file extension (`ttf` or `otf`).
- Deleting an imported font from the settings list deletes the corresponding file from disk.
- If an imported font file referenced in `layout.json` is missing or corrupted at startup, Yapfast logs a warning and falls back to default fonts without crashing.

### 2. Configuration Schema (`layout.json`)
Font settings are stored in `layout.json` under `fonts`:

```json
{
  "fonts": {
    "text": {
      "type": "builtin",
      "builtin": "dm_sans"
    },
    "emoji": {
      "type": "builtin",
      "builtin": "twemoji"
    },
    "custom": [
      {
        "id": "e81d8c06-1b0b-47e2-89e4-c5ec090bb6f1",
        "name": "Comic Neue",
        "family": "Comic Neue",
        "kind": "text",
        "filename": "e81d8c06-1b0b-47e2-89e4-c5ec090bb6f1.ttf"
      },
      {
        "id": "7a3f1201-4c22-48a9-9231-314159265358",
        "name": "JoyPixels",
        "family": "JoyPixels",
        "kind": "emoji",
        "filename": "7a3f1201-4c22-48a9-9231-314159265358.ttf"
      }
    ]
  }
}
```

### 3. Rust Data Structures (`crates/yapfast-iced/src/layout.rs` or `crates/yapfast-iced-kit/src/fonts.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinTextFont {
    #[default]
    DmSans,
    Inter,
    SystemDefault,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFontChoice {
    Builtin { builtin: BuiltinTextFont },
    Custom { id: String },
}

impl Default for TextFontChoice {
    fn default() -> Self {
        Self::Builtin {
            builtin: BuiltinTextFont::DmSans,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinEmojiFont {
    #[default]
    Twemoji,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmojiFontChoice {
    Builtin { builtin: BuiltinEmojiFont },
    Custom { id: String },
}

impl Default for EmojiFontChoice {
    fn default() -> Self {
        Self::Builtin {
            builtin: BuiltinEmojiFont::Twemoji,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomFontKind {
    Text,
    Emoji,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CustomFontEntry {
    pub id: String,
    pub name: String,
    pub family: String,
    pub kind: CustomFontKind,
    pub filename: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FontPrefs {
    pub text: TextFontChoice,
    pub emoji: EmojiFontChoice,
    pub custom: Vec<CustomFontEntry>,
}
```

---

## Font System & Dynamic Reshaping

### 1. Bundled Fonts
- `crates/yapfast-iced-kit/fonts/DMSans[opsz,wght].ttf` & `DMSans-Italic[opsz,wght].ttf` (existing).
- `crates/yapfast-iced-kit/fonts/Inter[opsz,wght].ttf` & `Inter-Italic[opsz,wght].ttf` (SIL Open Font License).
- Twemoji font (`Yapfast Twemoji`) generated at build time.

### 2. Generalizing Font Constants in `yapfast-iced-kit`
Currently, `yapfast-iced-kit::fonts` defines:
```rust
pub const REGULAR: Font = Font::with_name("DM Sans");
```
This is updated to:
```rust
pub const REGULAR: Font = Font {
    family: Family::SansSerif,
    weight: Weight::Normal,
    style: Style::Normal,
    stretch: Stretch::Normal,
};
pub const MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..REGULAR
};
pub const SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..REGULAR
};
pub const BOLD: Font = Font {
    weight: Weight::Bold,
    ..REGULAR
};
pub const ITALIC: Font = Font {
    style: Style::Italic,
    ..REGULAR
};
```
Because `REGULAR`, `BOLD`, `MEDIUM`, etc. now use `Family::SansSerif`, cosmic-text will query `db.sans_serif_family()`. When the active font changes, updating `db.set_sans_serif_family(&active_family)` changes the appearance of all standard UI widgets instantly without touching widget call sites.

### 3. Dynamic Fallback System
The static fallback `Fallback(&["DM Sans", TWEMOJI])` is upgraded to dynamically consult the active text and emoji families:
```rust
struct DynamicFallback;

impl cosmic_text::Fallback for DynamicFallback {
    fn common_fallback(&self) -> &[&'static str] {
        ACTIVE_FALLBACKS.lock().expect("active fallbacks").as_slice()
    }
    // ...
}
```
`ACTIVE_FALLBACKS` contains:
`[active_text_family, "DM Sans", active_emoji_family, "Yapfast Twemoji"]` (deduplicated).

### 4. Emoji Extraction in `paint.rs`
In `crates/yapfast-iced/src/paint.rs`, `twemoji_glyph` and `has_emoji` are updated:
- Query `active_emoji_family()` first; if missing, query `"Yapfast Twemoji"`.
- This ensures emoji picker items and reaction images render using the chosen emoji font.

### 5. Reshaping on Font Switch
When a font is selected or imported:
1. `yapfast_iced_kit::fonts::apply_fonts(...)` updates `sans_serif_family`, loads the file into `fontdb`, and updates the fallback list.
2. The font system version is bumped via:
   ```rust
   font_system().write().expect("font system").load_font(Cow::Owned(vec![0]));
   ```
3. A redraw is triggered (`cx.touch()`).
4. `layout.json` is saved via `cx.keep("fonts", &fonts_prefs)`.

---

## User Interface & Settings Panel

### 1. Appearance Page Integration
In `crates/yapfast-iced/src/settings/appearance.rs`, a new section `Fonts` is added to the scrollable view:

- **Text Font Subsection**:
  - Dropdown selector containing:
    - `DM Sans (Default)`
    - `Inter`
    - `System Default`
    - Separator
    - Custom fonts: each with a label and a delete/trash button.
  - "Import Text Font..." button opening a native file picker dialog (`rfd::AsyncFileDialog`) filtered to `.ttf` and `.otf`.

- **Emoji Font Subsection**:
  - Dropdown selector containing:
    - `Discord (Twemoji) (Default)`
    - Separator
    - Custom emoji fonts with label and delete/trash button.
  - "Import Emoji Font..." button opening `rfd::AsyncFileDialog` filtered to `.ttf` and `.otf`.

- **Interactive Preview Card**:
  - A stylized sample container showing:
    `"The quick brown fox jumps over the lazy dog. 🦊🎉❤️ 1234567890"`
  - Immediately demonstrates the live font rendering.

---

## Error Handling & Edge Cases

1. **Unsupported Font Format**: If `ttf-parser` or `fontdb` cannot parse the file, an error toast/banner is displayed and the file is not added.
2. **Missing Files on Boot**: If an imported font file referenced in `layout.json` no longer exists on disk, it is marked unavailable and the system gracefully falls back to `DM Sans` / `Twemoji`.
3. **Deletion of Active Font**: If the user deletes a custom font that is currently selected, the selection immediately reverts to `BuiltinTextFont::DmSans` or `BuiltinEmojiFont::Twemoji`.
4. **Memory Constraint**: Fonts are mapped or streamed; raw font byte clones are minimized to conform to Yapfast's memory targets.

---

## Verification & Testing

1. **Unit Tests**:
   - Serialization and deserialization of `FontPrefs` in `layout.rs`.
   - Font family name extraction via `ttf-parser`.
   - Fallback chain ordering with custom text and emoji fonts.
2. **Integration Tests**:
   - Import a valid `.ttf` font, verify it is copied to `<root>/fonts/` and registered in `layout.json`.
   - Delete an imported font, verify file removal and graceful fallback if active.
   - Run `cargo test --profile fast -p yapfast-iced-kit` and `cargo test --profile fast -p yapfast-iced`.
