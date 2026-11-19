# Custom Font & Emoji Font Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide custom font and emoji font support in Yapfast, allowing users to select among built-in fonts (DM Sans, Inter, System Default) or import custom `.ttf`/`.otf` text fonts and emoji fonts, with instant dynamic reshaping and persistence.

**Architecture:** Update `yapfast-iced-kit::fonts` to use `Family::SansSerif` for UI font constants (`REGULAR`, `BOLD`, etc.) and a dynamic fallback mechanism for emojis. Manage imported font files in `<root>/fonts/` and persist user selections and custom font records in `layout.json`. Add a Fonts section to the Appearance settings page with dropdown selectors, file import dialogs, delete actions, and a live preview card.

**Tech Stack:** Rust, Iced 0.14, cosmic-text, fontdb, ttf-parser, rfd (file dialogs), serde/serde_json.

**Spec:** `docs/superpowers/specs/2026-11-19-custom-fonts-design.md`

## Global Constraints

- Cargo commands must run in the dev shell with the fast profile: `nix develop path:./nix --command cargo test --profile fast -p <crate> <test>`.
- Incremental builds must be kept fast; compile and test only what changed.
- Avoid unnecessary memory allocations and copies (share data with `Arc` or borrows where appropriate).
- Commit messages must follow conventional commits without any Claude co-author attribution.

## Review Focus

1. **Corrupted or non-font file selected during import**: `ttf_parser::Face::parse` must fail cleanly before copying to `<root>/fonts/`, surfacing an error to the user without crashing.
2. **Missing font file referenced in `layout.json` at startup**: If a previously saved font file is deleted from disk externally, Yapfast must log a warning and fall back to DM Sans / Twemoji rather than failing to launch.
3. **Deletion of active font**: When a user deletes a custom font that is currently selected, the selection must immediately revert to the default font (DM Sans for text, Twemoji for emoji) and trigger a reshape.
4. **Emoji font lacking standard text glyphs**: Cosmic-text fallback must ensure text glyphs resolve to the active text font (or DM Sans) before falling back to the emoji font, so an emoji font containing blank or broken Latin glyphs does not corrupt UI text.
5. **Special characters or spaces in font family names**: Family names extracted from font metadata must be cleanly matched and sanitized when registering with `fontdb` and `cosmic-text`.

---

### Task 1: Core Font Configuration & Persistence Models

**Files:**
- Create: `crates/yapfast-core/src/fonts.rs`
- Modify: `crates/yapfast-core/src/lib.rs`
- Modify: `crates/yapfast-iced/src/layout.rs`
- Test: `crates/yapfast-core/src/fonts.rs` (inline tests)
- Test: `crates/yapfast-iced/src/tests/settings.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum BuiltinTextFont { DmSans, Inter, SystemDefault }
  pub enum TextFontChoice { Builtin { builtin: BuiltinTextFont }, Custom { id: String } }
  pub enum BuiltinEmojiFont { Twemoji }
  pub enum EmojiFontChoice { Builtin { builtin: BuiltinEmojiFont }, Custom { id: String } }
  pub enum CustomFontKind { Text, Emoji }
  pub struct CustomFontEntry { pub id: String, pub name: String, pub family: String, pub kind: CustomFontKind, pub filename: String }
  pub struct FontPrefs { pub text: TextFontChoice, pub emoji: EmojiFontChoice, pub custom: Vec<CustomFontEntry> }
  ```

- [ ] **Step 1: Write the failing tests for FontPrefs serialization and default fallback**

In `crates/yapfast-core/src/fonts.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_prefs_defaults_to_dm_sans_and_twemoji() {
        let prefs = FontPrefs::default();
        assert_eq!(prefs.text, TextFontChoice::Builtin { builtin: BuiltinTextFont::DmSans });
        assert_eq!(prefs.emoji, EmojiFontChoice::Builtin { builtin: BuiltinEmojiFont::Twemoji });
        assert!(prefs.custom.is_empty());
    }

    #[test]
    fn font_prefs_serializes_and_deserializes() {
        let mut prefs = FontPrefs::default();
        prefs.custom.push(CustomFontEntry {
            id: "uuid-123".into(),
            name: "Fira Code".into(),
            family: "Fira Code".into(),
            kind: CustomFontKind::Text,
            filename: "uuid-123.ttf".into(),
        });
        prefs.text = TextFontChoice::Custom { id: "uuid-123".into() };
        let json = serde_json::to_string(&prefs).expect("serialize");
        let parsed: FontPrefs = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(prefs, parsed);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-core fonts`
Expected: FAIL (file or types not yet defined)

- [ ] **Step 3: Implement data structures in `crates/yapfast-core/src/fonts.rs` and integrate into `layout.rs`**

- Define `BuiltinTextFont`, `TextFontChoice`, `BuiltinEmojiFont`, `EmojiFontChoice`, `CustomFontKind`, `CustomFontEntry`, and `FontPrefs` with serde derive macros in `crates/yapfast-core/src/fonts.rs`.
- Re-export in `crates/yapfast-core/src/lib.rs`.
- In `crates/yapfast-iced/src/layout.rs`, add `pub fonts: FontPrefs` field to `Layout` (or `Prefs`) with `#[serde(default)]`.

- [ ] **Step 4: Run test to verify it passes**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-core fonts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-core/src/fonts.rs crates/yapfast-core/src/lib.rs crates/yapfast-iced/src/layout.rs
git commit -m "feat(fonts): add font preferences and configuration models"
```

---

### Task 2: Bundle Inter Font & Generalize Font System in `yapfast-iced-kit`

**Files:**
- Create: `crates/yapfast-iced-kit/fonts/Inter[opsz,wght].ttf`
- Create: `crates/yapfast-iced-kit/fonts/Inter-Italic[opsz,wght].ttf`
- Create: `crates/yapfast-iced-kit/fonts/LICENSE-OFL-Inter.txt`
- Modify: `crates/yapfast-iced-kit/src/fonts.rs`
- Test: `crates/yapfast-iced-kit/src/fonts.rs` (inline tests)

**Interfaces:**
- Consumes: `FontPrefs`
- Produces:
  ```rust
  pub const REGULAR: Font; // using Family::SansSerif
  pub const MEDIUM: Font;
  pub const SEMIBOLD: Font;
  pub const BOLD: Font;
  pub const ITALIC: Font;
  pub const INTER: &str = "Inter";
  pub fn apply_fonts(active_text: Option<&str>, active_emoji: Option<&str>, custom_files: &[(String, std::path::PathBuf)]) -> bool;
  pub fn active_emoji_family() -> &'static str;
  ```

- [ ] **Step 1: Write the failing test for dynamic font selection & fallback**

In `crates/yapfast-iced-kit/src/fonts.rs`:
```rust
#[test]
fn applying_custom_font_updates_sans_serif_family_and_version() {
    crate::testing::install_fonts();
    let initial_version = font_system().read().expect("font system").version();
    let applied = apply_fonts(Some("Inter"), Some(TWEMOJI), &[]);
    assert!(applied);
    let new_version = font_system().read().expect("font system").version();
    assert_ne!(initial_version, new_version);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced-kit applying_custom_font`
Expected: FAIL (apply_fonts not defined)

- [ ] **Step 3: Implement font bundling, Family::SansSerif constants, and dynamic fallback in `fonts.rs`**

- Fetch and place `Inter[opsz,wght].ttf`, `Inter-Italic[opsz,wght].ttf`, and `LICENSE-OFL-Inter.txt` under `crates/yapfast-iced-kit/fonts/`.
- Include Inter bytes: `const INTER_FONT: &[u8] = include_bytes!("../fonts/Inter[opsz,wght].ttf");` and `const INTER_FONT_ITALIC: &[u8] = include_bytes!("../fonts/Inter-Italic[opsz,wght].ttf");`.
- Update `REGULAR`, `MEDIUM`, `SEMIBOLD`, `BOLD`, `ITALIC` to use `family: Family::SansSerif`.
- Convert static `Fallback` in `fonts.rs` to a dynamic fallback holding active text and emoji families behind a static mutex (`ACTIVE_FALLBACKS`).
- Implement `apply_fonts` to update `db.set_sans_serif_family(...)`, load any custom font files via `db.load_font_file(...)`, update `ACTIVE_FALLBACKS`, and bump font system version via `load_font(Cow::Owned(vec![0]))`.

- [ ] **Step 4: Run test to verify it passes**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced-kit applying_custom_font`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-iced-kit/fonts/ crates/yapfast-iced-kit/src/fonts.rs
git commit -m "feat(fonts): bundle Inter and generalize font constants with dynamic fallback"
```

---

### Task 3: Font File Manager & Validation Module

**Files:**
- Create: `crates/yapfast-iced/src/font_manager.rs`
- Modify: `crates/yapfast-iced/src/lib.rs`
- Test: `crates/yapfast-iced/src/font_manager.rs` (inline tests)

**Interfaces:**
- Consumes: `CustomFontEntry`, `CustomFontKind`, `FontPrefs`
- Produces:
  ```rust
  pub fn inspect_font_file(path: &std::path::Path) -> Result<(String, bool), String>;
  pub fn import_font(root: &std::path::Path, source_path: &std::path::Path, kind: CustomFontKind) -> Result<CustomFontEntry, String>;
  pub fn delete_font(root: &std::path::Path, entry: &CustomFontEntry) -> Result<(), std::io::Error>;
  pub fn fonts_directory(root: &std::path::Path) -> std::path::PathBuf;
  ```

- [ ] **Step 1: Write the failing tests for inspect_font_file and import_font**

In `crates/yapfast-iced/src/font_manager.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_valid_dm_sans_font() {
        let path = std::path::Path::new("crates/yapfast-iced-kit/fonts/DMSans[opsz,wght].ttf");
        let (family, has_color) = inspect_font_file(path).expect("valid font");
        assert_eq!(family, "DM Sans");
        assert!(!has_color);
    }

    #[test]
    fn reject_corrupted_font() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"not a font at all").expect("write");
        let result = inspect_font_file(temp.path());
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced inspect_valid_dm_sans`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `font_manager.rs`**

- Use `ttf_parser::Face::parse` to extract the English family name from font name records and detect CBDT/CBLC, sbix, or COLR tables for color emoji.
- Implement `import_font`: validates the source file, creates `<root>/fonts/` directory, assigns a UUID v4, copies the file, and returns a `CustomFontEntry`.
- Implement `delete_font`: removes the file from `<root>/fonts/`.

- [ ] **Step 4: Run test to verify it passes**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced inspect_valid_dm_sans reject_corrupted_font`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-iced/src/font_manager.rs crates/yapfast-iced/src/lib.rs
git commit -m "feat(fonts): add font file manager, metadata extraction, and validation"
```

---

### Task 4: Generalized Emoji Rendering in `paint.rs`

**Files:**
- Modify: `crates/yapfast-iced/src/paint.rs`
- Test: `crates/yapfast-iced/src/paint.rs` (inline tests)

**Interfaces:**
- Consumes: `yapfast_iced_kit::fonts::active_emoji_family()`
- Produces:
  ```rust
  pub fn emoji(system: &mut cosmic_text::FontSystem, emoji: &str, size: u32) -> Option<Canvas>;
  pub fn has_emoji(system: &mut cosmic_text::FontSystem, emoji: &str) -> bool;
  ```

- [ ] **Step 1: Write the failing test for emoji rendering with active emoji font**

In `crates/yapfast-iced/src/paint.rs`:
```rust
#[test]
fn renders_emoji_with_active_emoji_font() {
    let mut system = iced::advanced::graphics::text::font_system().write().expect("font system");
    assert!(has_emoji(system.raw(), "❤️"));
    let canvas = emoji(system.raw(), "❤️", 32).expect("emoji canvas");
    assert_eq!(canvas.width, 32);
    assert_eq!(canvas.height, 32);
}
```

- [ ] **Step 2: Run test to verify it fails or runs**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced renders_emoji_with_active_emoji_font`
Expected: FAIL or passes with hardcoded Twemoji only.

- [ ] **Step 3: Update `twemoji_glyph` in `paint.rs` to support the active emoji family**

- In `twemoji_glyph(system: &mut cosmic_text::FontSystem, emoji: &str, size: f32)`:
  - Query with `Family::Name(yapfast_iced_kit::fonts::active_emoji_family())` first.
  - If no glyph is found and active family != `"Yapfast Twemoji"`, fall back to querying `"Yapfast Twemoji"`.
  - Extract the color image using Swash and resample to canvas dimensions.

- [ ] **Step 4: Run test to verify it passes**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced renders_emoji_with_active_emoji_font`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-iced/src/paint.rs
git commit -m "feat(fonts): generalize emoji glyph extraction to support active emoji fonts"
```

---

### Task 5: Settings UI - Fonts Section in Appearance Page

**Files:**
- Modify: `crates/yapfast-iced/src/settings/appearance.rs`
- Modify: `crates/yapfast-iced/src/settings/mod.rs`
- Test: `crates/yapfast-iced/src/tests/settings.rs`

**Interfaces:**
- Consumes: `font_manager`, `yapfast_iced_kit::fonts::apply_fonts`, `FontPrefs`
- Produces:
  ```rust
  pub enum Event {
      // existing variants...
      ChooseTextFont(TextFontChoice),
      ChooseEmojiFont(EmojiFontChoice),
      ImportFont(CustomFontKind),
      FontImported(Result<CustomFontEntry, String>),
      DeleteCustomFont(String),
  }
  ```

- [ ] **Step 1: Write the failing tests for Appearance font settings events**

In `crates/yapfast-iced/src/tests/settings.rs`:
```rust
#[test]
fn choosing_text_font_updates_layout_and_font_system() {
    let (mut app, _rx) = test_app();
    // Dispatch choosing Inter
    app.update(appearance_event(ThemeEvent::ChooseTextFont(TextFontChoice::Builtin {
        builtin: BuiltinTextFont::Inter,
    })));
    assert_eq!(
        app.layout.fonts.text,
        TextFontChoice::Builtin { builtin: BuiltinTextFont::Inter }
    );
}

#[test]
fn deleting_active_font_reverts_to_default() {
    let (mut app, _rx) = test_app();
    let id = "test-custom-id".to_string();
    app.layout.fonts.custom.push(CustomFontEntry {
        id: id.clone(),
        name: "Test Font".into(),
        family: "Test Font".into(),
        kind: CustomFontKind::Text,
        filename: "test.ttf".into(),
    });
    app.layout.fonts.text = TextFontChoice::Custom { id: id.clone() };
    app.update(appearance_event(ThemeEvent::DeleteCustomFont(id)));
    assert_eq!(
        app.layout.fonts.text,
        TextFontChoice::Builtin { builtin: BuiltinTextFont::DmSans }
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced choosing_text_font_updates_layout`
Expected: FAIL (events not yet implemented)

- [ ] **Step 3: Implement Fonts section and event handlers in `appearance.rs`**

- Add `ChooseTextFont`, `ChooseEmojiFont`, `ImportFont`, `FontImported`, `DeleteCustomFont` to `appearance::Event`.
- In `theme_event`, handle:
  - `ChooseTextFont`: updates `cx.layout.fonts.text`, saves via `cx.keep("fonts", ...)`, calls `apply_fonts`, and calls `cx.touch()`.
  - `ChooseEmojiFont`: updates `cx.layout.fonts.emoji`, saves, calls `apply_fonts`, and calls `cx.touch()`.
  - `ImportFont(kind)`: launches `rfd::AsyncFileDialog` task filtered to `.ttf`/`.otf`, imports via `font_manager::import_font`, and returns `FontImported`.
  - `FontImported`: on success, appends to `cx.layout.fonts.custom`, auto-selects it, calls `apply_fonts`, saves, and touches.
  - `DeleteCustomFont(id)`: deletes file via `font_manager::delete_font`, removes entry from `custom`, reverts active font to default if it was the deleted one, calls `apply_fonts`, saves, and touches.
- In `appearance::view`, render:
  - Text Font section with dropdown selector listing built-ins (DM Sans, Inter, System Default) and custom fonts, with delete buttons for custom fonts.
  - "Import Font..." button.
  - Emoji Font section with dropdown selector listing Twemoji and custom emoji fonts, with delete buttons.
  - "Import Emoji Font..." button.
  - Live Font Preview Card displaying `"The quick brown fox jumps over the lazy dog. 🦊🎉❤️ 1234567890"`.

- [ ] **Step 4: Run test to verify it passes**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced choosing_text_font_updates_layout deleting_active_font_reverts_to_default`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-iced/src/settings/appearance.rs crates/yapfast-iced/src/settings/mod.rs crates/yapfast-iced/src/tests/settings.rs
git commit -m "feat(settings): add Fonts section to Appearance page with import, selection, and preview"
```

---

### Task 6: App Startup Font Initialization & End-to-End Verification

**Files:**
- Modify: `crates/yapfast-iced/src/lib.rs`
- Modify: `crates/yapfast-iced/src/app/mod.rs`
- Test: `crates/yapfast-iced/src/tests/settings.rs`

**Interfaces:**
- Consumes: `Layout::fonts`, `yapfast_iced_kit::fonts::apply_fonts`, `font_manager`

- [ ] **Step 1: Write integration test for startup font loading from layout.json**

In `crates/yapfast-iced/src/tests/settings.rs`:
```rust
#[test]
fn loads_saved_fonts_on_startup() {
    let folder = tempfile::tempdir().expect("temp folder");
    let layout_json = r#"{
        "fonts": {
            "text": { "type": "builtin", "builtin": "inter" },
            "emoji": { "type": "builtin", "builtin": "twemoji" },
            "custom": []
        }
    }"#;
    std::fs::write(folder.path().join("layout.json"), layout_json).expect("write layout");
    let layout = crate::layout::Layout::read(folder.path());
    assert_eq!(layout.fonts.text, TextFontChoice::Builtin { builtin: BuiltinTextFont::Inter });
}
```

- [ ] **Step 2: Run test to verify it fails or runs**

Run: `nix develop path:./nix --command cargo test --profile fast -p yapfast-iced loads_saved_fonts_on_startup`
Expected: PASS or FAIL depending on Layout::read deserialization.

- [ ] **Step 3: Initialize saved fonts on app boot in `crates/yapfast-iced/src/lib.rs` and `App::new`**

- In `crates/yapfast-iced/src/lib.rs` (around `fonts::install(TWEMOJI)`):
  - Read `Layout` or inspect `root/layout.json`.
  - Apply the saved fonts configuration (active text, active emoji, and custom font file paths from `<root>/fonts/`).
- If any custom font file is missing, log a warning and fallback gracefully to DM Sans / Twemoji.

- [ ] **Step 4: Run full verification suite**

Run:
```bash
nix develop path:./nix --command cargo test --profile fast -p yapfast-core fonts
nix develop path:./nix --command cargo test --profile fast -p yapfast-iced-kit
nix develop path:./nix --command cargo test --profile fast -p yapfast-iced settings
```
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/yapfast-iced/src/lib.rs crates/yapfast-iced/src/app/mod.rs crates/yapfast-iced/src/tests/settings.rs
git commit -m "feat(fonts): apply saved font settings on app startup"
```
