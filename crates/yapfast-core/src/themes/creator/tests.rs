use super::*;
use crate::themes::check::{Files, Strictness, check};

fn font(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "AtkinsonHyperlegibleNext-Regular.ttf" => {
            include_bytes!(
                "../../../../../crates/yapfast-iced-kit/fonts/AtkinsonHyperlegibleNext-Regular.ttf"
            )
        }
        "DisplayName-Tempo.ttf" => {
            include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Tempo.ttf")
        }
        "DisplayName-Modern.ttf" => {
            include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Modern.ttf")
        }
        "DisplayName-MonkeyBars.ttf" => {
            include_bytes!(
                "../../../../../crates/yapfast-iced-kit/fonts/DisplayName-MonkeyBars.ttf"
            )
        }
        "DisplayName-Journal.ttf" => {
            include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Journal.ttf")
        }
        "DisplayName-8Bit.ttf" => {
            include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-8Bit.ttf")
        }
        "DisplayName-Mainframe.ttf" => {
            include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Mainframe.ttf")
        }
        _ => return None,
    })
}

/// Writes a spec's theme into a fresh folder and runs the checks an install
/// runs on it, returning why it's refused, if it is.
fn refused(spec: &Spec, picture: Option<&[u8]>) -> Option<String> {
    let folder = tempfile::tempdir().expect("a temporary folder");
    if let Some(jpeg) = picture {
        store_picture(folder.path(), jpeg).expect("kept");
    }
    let derived = derive(spec);
    write_folder(folder.path(), spec, "Test", "custom:1", &derived, font).expect("written");
    let files = Files::read(folder.path()).expect("read back");
    let report = check(&files, Strictness::Install);
    report.reason().map(str::to_owned)
}

fn solid(width: u32, height: u32, pick: impl Fn(u32, u32) -> [u8; 3]) -> image::DynamicImage {
    image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb(pick(x, y))
    }))
}

fn jpeg(picture: &image::DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    picture
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Jpeg,
        )
        .expect("encodes");
    bytes
}

fn hue_of(colour: u32) -> f64 {
    lch(colour)[2]
}

fn hue_near(colour: u32, hue: f64, within: f64) -> bool {
    ((hue_of(colour) - hue + 540.0) % 360.0 - 180.0).abs() <= within
}

#[test]
fn oklch_round_trips() {
    for colour in [
        0x000000, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0xffbf47, 0x1e1f24, 0x7a3fd1,
    ] {
        let back = from_lch(lch(colour));
        for shift in [16, 8, 0] {
            let (one, two) = ((colour >> shift) & 0xff, (back >> shift) & 0xff);
            assert!(one.abs_diff(two) <= 1, "{colour:06x} came back {back:06x}");
        }
    }
}

#[test]
fn a_palette_finds_both_halves_of_a_picture() {
    let picture = solid(200, 100, |x, _| {
        if x < 120 {
            [0xe8, 0x70, 0x2a]
        } else {
            [0x1b, 0x2a, 0x4a]
        }
    });
    let found = palette(&picture);
    let orange = lch(0xe8702a)[2];
    let navy = lch(0x1b2a4a)[2];
    assert!(
        found
            .colours
            .iter()
            .any(|colour| hue_near(*colour, orange, 15.0)),
        "{found:x?}"
    );
    assert!(
        found
            .colours
            .iter()
            .any(|colour| hue_near(*colour, navy, 20.0)),
        "{found:x?}"
    );
    // The lively one is the accent; the dark one tints the grounds.
    assert!(
        hue_near(found.accent, orange, 15.0),
        "accent {:06x}",
        found.accent
    );
    assert!(
        hue_near(found.ground, navy, 20.0),
        "ground {:06x}",
        found.ground
    );
    assert!(lch(found.ground)[0] < lch(found.accent)[0]);
    // The most used comes first.
    assert!(hue_near(found.colours[0], orange, 15.0));
}

#[test]
fn a_palette_is_the_same_every_time() {
    let picture = solid(97, 61, |x, y| {
        [(x * 2) as u8, (y * 4) as u8, ((x + y) % 255) as u8]
    });
    assert_eq!(palette(&picture), palette(&picture));
}

#[test]
fn a_sunset_gives_a_warm_accent_and_a_dusky_ground() {
    // Deep violet sky over an orange horizon and a dark sea.
    let picture = solid(320, 200, |_, y| {
        let t = y as f32 / 200.0;
        let mix = |a: f32, b: f32, t: f32| (a + (b - a) * t) as u8;
        if t < 0.6 {
            let t = t / 0.6;
            [mix(48.0, 250.0, t), mix(30.0, 120.0, t), mix(90.0, 60.0, t)]
        } else {
            let t = (t - 0.6) / 0.4;
            [mix(120.0, 20.0, t), mix(50.0, 15.0, t), mix(60.0, 40.0, t)]
        }
    });
    let prepared = prepare_picture(&jpeg(&picture)).expect("a picture");
    let [_, chroma, hue] = lch(prepared.accent);
    assert!(
        (15.0..90.0).contains(&hue),
        "accent {:06x} hue {hue}",
        prepared.accent
    );
    assert!(chroma >= 0.09, "a lively accent");
    assert!(
        lch(prepared.ground)[0] < 0.45,
        "ground {:06x}",
        prepared.ground
    );
    assert!(!prepared.picture.palette.is_empty() && prepared.picture.palette.len() <= 6);
    assert!(prepared.jpeg.len() as u64 <= MAX_IMAGE_BYTES);
}

#[test]
fn a_large_picture_is_scaled_to_fit_a_theme() {
    let picture = solid(4000, 2500, |x, y| {
        [(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 239) as u8]
    });
    let prepared = prepare_picture(&jpeg(&picture)).expect("a picture");
    let stored = image::load_from_memory(&prepared.jpeg).expect("decodes");
    assert_eq!(stored.width().max(stored.height()), 1920);
    assert!(prepared.jpeg.len() as u64 <= MAX_IMAGE_BYTES);
    assert!(check::check_image(&prepared.jpeg).is_ok());
}

#[test]
fn something_that_isnt_a_picture_is_refused_plainly() {
    let refused = prepare_picture(b"not a picture at all")
        .err()
        .expect("refused");
    assert_eq!(refused, "That file isn't a picture Yapfast can use");
}

#[test]
fn every_derived_theme_passes_the_contrast_checks() {
    // Accents round the wheel at several lightnesses and strengths, on dark
    // and light grounds tinted every way, with every message style and with
    // and without glass: each must install, with no contrast refused.
    let mut tried = 0;
    for hue in (0..360).step_by(30) {
        for (lightness, chroma) in [
            (0.45, 0.15),
            (0.65, 0.2),
            (0.85, 0.12),
            (0.97, 0.03),
            (0.25, 0.08),
        ] {
            for ground in [0x1e1f24, 0x2a1a3a, 0x0b3d2e, 0x5a3b10, 0xf0e6ff, 0x808080] {
                for background in [Background::Dark, Background::Light] {
                    let index = tried;
                    tried += 1;
                    let spec = Spec {
                        accent: from_lch([lightness, chroma, f64::from(hue)]),
                        ground,
                        background,
                        messages: [
                            Messages::Flat,
                            Messages::Bubbles,
                            Messages::Cards,
                            Messages::Irc,
                        ][index % 4],
                        glass: index % 3 == 0,
                        corners: (index * 37 % 101) as u8,
                        density: [Density::Compact, Density::Cozy, Density::Roomy][index % 3],
                        ..Spec::default()
                    };
                    assert_eq!(refused(&spec, None), None, "{spec:?}");
                }
            }
        }
    }
    assert!(tried >= 700);
}

#[test]
fn derived_inks_read_on_every_ground_they_sit_on() {
    for hue in (0..360).step_by(15) {
        for background in [Background::Dark, Background::Light] {
            let spec = Spec {
                accent: from_lch([0.7, 0.18, f64::from(hue)]),
                ground: from_lch([0.3, 0.06, f64::from((hue + 120) % 360)]),
                background,
                ..Spec::default()
            };
            let derived = derive(&spec);
            let get = |token: &str| derived.colour(token).expect(token);
            for ink in ["text", "secondary", "muted", "link", "accent-text"] {
                for ground in ["sidebar", "panel", "chat", "field"] {
                    let reads = contrast(get(ink), get(ground));
                    assert!(
                        reads >= READABLE,
                        "{ink} on {ground}: {reads:.2} ({spec:?})"
                    );
                }
            }
            assert!(contrast(get("on-accent"), get("accent")) >= READABLE);
            assert!(contrast(get("text"), get("raised")) >= READABLE);
            // The accent is the person's, untouched.
            assert_eq!(get("accent") & 0xff_ffff, spec.accent);
        }
    }
}

#[test]
fn text_reads_over_any_picture_with_or_without_glass() {
    let pictures = [
        solid(64, 64, |_, _| [255, 255, 255]),
        solid(64, 64, |_, _| [128, 128, 128]),
        solid(64, 64, |_, _| [250, 220, 40]),
        solid(64, 64, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            }
        }),
        solid(64, 64, |_, _| [10, 10, 30]),
    ];
    for picture in &pictures {
        let prepared = prepare_picture(&jpeg(picture)).expect("a picture");
        for glass in [false, true] {
            for dim in [0, 35, 100] {
                let spec = Spec {
                    accent: prepared.accent,
                    ground: prepared.ground,
                    background: Background::Picture,
                    picture: Some(prepared.picture.clone()),
                    glass,
                    dim,
                    ..Spec::default()
                };
                assert_eq!(refused(&spec, Some(&prepared.jpeg)), None, "{spec:?}");
                // Dimmed at least as much as asked.
                assert!(derive(&spec).dim + 0.001 >= f32::from(dim) / 100.0 * 0.85);
            }
        }
    }
}

#[test]
fn a_nudged_ink_always_reads() {
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed & 0xff_ffff
    };
    for _ in 0..2000 {
        let (ink, one, two) = (next(), next(), next());
        let lighter = lch(one)[0] < 0.5;
        let grounds = [one, two];
        let nudged = nudge(ink, &grounds, 4.6, lighter);
        // Grounds far apart may have no ink that reads on both; one side
        // always has one.
        if worst(if lighter { 0xffffff } else { 0x000000 }, &grounds) >= 4.6 {
            assert!(
                worst(nudged, &grounds) >= 4.6,
                "{ink:06x} on {one:06x}, {two:06x}"
            );
        }
    }
}

#[test]
fn corners_run_from_square_to_round_with_stock_in_the_middle() {
    assert_eq!(
        shape(&Spec {
            corners: 50,
            ..Spec::default()
        }),
        None
    );
    let square = shape(&Spec {
        corners: 0,
        ..Spec::default()
    })
    .expect("a shape");
    assert_eq!(square["button"]["radius"], json!(0.0));
    assert_eq!(square["avatar"]["radius"], json!(0.0));
    assert_eq!(square["radii"]["large"], json!(0.0));
    let round = shape(&Spec {
        corners: 100,
        ..Spec::default()
    })
    .expect("a shape");
    assert_eq!(round["button"]["radius"], json!(999.0));
    assert_eq!(round["avatar"]["radius"], json!(50.0));
    assert_eq!(round["card"]["radius"], json!(22.0));
    let near = shape(&Spec {
        corners: 49,
        ..Spec::default()
    })
    .expect("a shape");
    assert!((near["card"]["radius"].as_f64().unwrap() - 12.0).abs() <= 0.5);
    // Outlines other than Yapfast's own make a shape at stock corners too.
    let outlined = shape(&Spec {
        outline: 3,
        ..Spec::default()
    })
    .expect("a shape");
    assert_eq!(outlined["outline_width"], json!(3));
}

#[test]
fn the_default_spec_is_a_plain_theme() {
    let spec = Spec::default();
    let text = manifest(&spec, "Mine", "custom:4", &derive(&spec));
    let value: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(value["id"], "local.creator.custom-4");
    for layer in ["shape", "type", "spacing", "layout", "art"] {
        assert!(value.get(layer).is_none(), "{layer} set by default");
    }
    assert_eq!(refused(&spec, None), None);
}

#[test]
fn every_built_in_font_ships_and_passes() {
    for font in FONTS {
        let spec = Spec {
            body: font.key.to_owned(),
            headings: Some("tech".to_owned()),
            ..Spec::default()
        };
        assert_eq!(refused(&spec, None), None, "{}", font.key);
    }
}

#[test]
fn a_spec_round_trips_through_layout_json() {
    let spec = Spec {
        messages: Messages::Irc,
        background: Background::Picture,
        picture: Some(Picture {
            mean: 0x223344,
            palette: vec![0xff0000],
        }),
        headings: Some("pixel".to_owned()),
        ..Spec::default()
    };
    let text = serde_json::to_string(&spec).expect("serialises");
    assert_eq!(serde_json::from_str::<Spec>(&text).expect("reads"), spec);
    // Missing fields take their defaults.
    let old: Spec = serde_json::from_str(r#"{ "accent": 255 }"#).expect("reads");
    assert_eq!(old.accent, 255);
    assert_eq!(old.corners, 50);
}

#[test]
fn a_file_that_isnt_a_font_is_refused_plainly() {
    let refused = inspect_font(b"definitely not a font").expect_err("refused");
    assert_eq!(refused, NOT_A_FONT);
    let png = jpeg(&solid(4, 4, |_, _| [1, 2, 3]));
    assert!(
        inspect_font(&png)
            .expect_err("refused")
            .starts_with(NOT_A_FONT)
    );
    let large = vec![0u8; (MAX_FONT_BYTES + 1) as usize];
    assert!(
        inspect_font(&large)
            .expect_err("refused")
            .contains("larger than 4 MB")
    );
}

#[test]
fn a_font_is_read_for_its_name_licence_and_letters() {
    let facts =
        inspect_font(font("AtkinsonHyperlegibleNext-Regular.ttf").unwrap()).expect("a font");
    assert_eq!(facts.family, "Atkinson Hyperlegible Next");
    assert_eq!(facts.extension, "ttf");
    assert!(
        !facts.partial,
        "Atkinson Hyperlegible Next draws all of basic Latin"
    );
    // Néo-castel has no ^ or ~ (crates/yapfast-iced-kit/fonts/README.md).
    let medieval =
        include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Medieval.otf");
    let facts = inspect_font(medieval).expect("a font");
    assert!(facts.partial);
    assert_eq!(facts.extension, "otf");
}

#[test]
fn an_imported_font_is_copied_into_the_theme_and_carried_by_it() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let bytes =
        include_bytes!("../../../../../crates/yapfast-iced-kit/fonts/DisplayName-Medieval.otf");
    let imported = store_font(folder.path(), bytes).expect("kept");
    assert!(
        imported.file.starts_with("fonts/") && imported.file.ends_with(".otf"),
        "{}",
        imported.file
    );
    assert_eq!(
        std::fs::read(folder.path().join(&imported.file)).expect("copied"),
        bytes
    );
    let spec = Spec {
        body: "readable".to_owned(),
        headings: Some(imported.file.clone()),
        fonts: vec![imported.clone()],
        ..Spec::default()
    };
    let derived = derive(&spec);
    write_folder(folder.path(), &spec, "Castle", "custom:2", &derived, font).expect("written");
    let files = Files::read(folder.path()).expect("read back");
    assert!(
        files.files.contains_key(Path::new(&imported.file)),
        "the theme names its font"
    );
    assert_eq!(check(&files, Strictness::Install).reason(), None);
    let value: Value = serde_json::from_str(&files.manifest).expect("JSON");
    assert_eq!(value["type"]["headings"]["font"], json!(imported.file));
    assert!(
        value["description"]
            .as_str()
            .unwrap()
            .contains(&imported.family)
    );
    let licence = std::fs::read_to_string(folder.path().join("LICENSE")).expect("a licence");
    assert!(licence.contains(&imported.file) && licence.contains("allowed to share"));
    // A copy, as an export makes, carries the font too.
    let export = tempfile::tempdir().expect("a temporary folder");
    export_folder(folder.path(), &export.path().join("castle")).expect("copied");
    assert!(export.path().join("castle").join(&imported.file).is_file());
}

#[test]
fn an_export_never_removes_what_is_already_there() {
    let theme = tempfile::tempdir().expect("a temporary folder");
    std::fs::write(theme.path().join("theme.json"), "{}").expect("written");
    let home = tempfile::tempdir().expect("a temporary folder");
    let projects = home.path().join("projects");
    std::fs::create_dir(&projects).expect("made");
    std::fs::write(projects.join("notes.txt"), "mine").expect("written");
    assert!(export_folder(theme.path(), &projects).is_err());
    assert_eq!(
        std::fs::read_to_string(projects.join("notes.txt")).expect("kept"),
        "mine"
    );
    assert!(!projects.join("theme.json").exists());
    // A file of that name is refused too; an empty folder takes the export.
    std::fs::write(home.path().join("file"), "x").expect("written");
    assert!(export_folder(theme.path(), &home.path().join("file")).is_err());
    std::fs::create_dir(home.path().join("empty")).expect("made");
    export_folder(theme.path(), &home.path().join("empty")).expect("exported");
    assert!(home.path().join("empty").join("theme.json").is_file());
}

#[test]
fn a_save_that_fails_halfway_keeps_the_saved_theme() {
    let saved = tempfile::tempdir().expect("a temporary folder");
    let target = saved.path().join("1");
    std::fs::create_dir(&target).expect("made");
    std::fs::write(target.join("theme.json"), "saved").expect("written");
    // A draft that isn't there can't be copied: the saved theme stays.
    let missing = saved.path().join("no draft");
    assert!(copy_folder(&missing, &target).is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("theme.json")).expect("kept"),
        "saved"
    );
    assert!(!saved.path().join("1.incoming").exists());
    // A draft that is replaces it whole.
    let draft = tempfile::tempdir().expect("a temporary folder");
    std::fs::write(draft.path().join("theme.json"), "new").expect("written");
    copy_folder(draft.path(), &target).expect("copied");
    assert_eq!(
        std::fs::read_to_string(target.join("theme.json")).expect("read"),
        "new"
    );
}

#[test]
fn a_font_that_isnt_one_is_never_copied() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    assert!(store_font(folder.path(), b"nope").is_err());
    assert!(!folder.path().join("fonts").exists());
}

#[test]
fn hovers_are_washes_that_keep_text_readable_on_every_ground() {
    // A picture's colours as the creator reads them: bright, middling and
    // dark, and the grey-brown of the theme that showed a solid slab.
    let pictures: Vec<(PreparedPicture, Vec<u8>)> = [
        solid(64, 64, |_, _| [250, 245, 235]),
        solid(64, 64, |_, _| [128, 120, 118]),
        solid(64, 64, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 {
                [255, 255, 255]
            } else {
                [20, 18, 18]
            }
        }),
        solid(64, 64, |_, _| [0x1f, 0x1e, 0x1e]),
    ]
    .iter()
    .map(|picture| {
        let bytes = jpeg(picture);
        (prepare_picture(&bytes).expect("a picture"), bytes)
    })
    .collect();
    let mut tried = 0;
    for hue in (0..360).step_by(45) {
        for (lightness, chroma) in [(0.45, 0.15), (0.7, 0.18), (0.95, 0.04)] {
            for ground in [0x1e1f24, 0x1f1e1e, 0x2a1a3a, 0x0b3d2e, 0xf0e6ff] {
                for background in [Background::Dark, Background::Light, Background::Picture] {
                    for glass in [false, true] {
                        let index = tried;
                        tried += 1;
                        let picture = &pictures[index % pictures.len()];
                        let spec = Spec {
                            accent: from_lch([lightness, chroma, f64::from(hue)]),
                            ground,
                            background,
                            glass,
                            dim: [0, 35, 100][index % 3],
                            picture: (background == Background::Picture)
                                .then(|| picture.0.picture.clone()),
                            ..Spec::default()
                        };
                        let derived = derive(&spec);
                        let get = |token: &str| derived.colour(token).expect(token);
                        // Washes, not grounds: see-through, white on dark and
                        // black on light, so a picture shows through a hovered
                        // row.
                        for wash in WASHES {
                            let alpha = get(wash) >> 24;
                            assert!(
                                (0x08..=0x20).contains(&alpha),
                                "{wash} {:08x} ({spec:?})",
                                get(wash)
                            );
                            let rgb = get(wash) & 0xff_ffff;
                            assert_eq!(
                                rgb,
                                if derived.dark { 0xffffff } else { 0x000000 },
                                "{wash} ({spec:?})"
                            );
                        }
                        // Text on a hovered row reads, over each ground…
                        for wash in WASHES {
                            for surface in ["sidebar", "panel", "chat", "field", "raised"] {
                                let under = washed(get(wash), get(surface));
                                for ink in ["text", "secondary", "muted", "link", "accent-text"] {
                                    let reads = contrast(get(ink), under);
                                    assert!(
                                        reads >= READABLE,
                                        "{ink} on {wash} over {surface}: {reads:.2} ({spec:?})"
                                    );
                                }
                            }
                        }
                        // …and over the art, as `check::art` measures it.
                        if let Some(mean) = art_mean(&spec) {
                            let art = dimmed(mean, derived.dim);
                            let unders: Vec<u32> = if spec.glass {
                                ["sidebar", "chat", "panel"]
                                    .iter()
                                    .map(|surface| mix(art, get(surface), derived.glass_opacity))
                                    .collect()
                            } else {
                                vec![art]
                            };
                            for under in unders {
                                for wash in WASHES {
                                    let lit = washed(get(wash), under);
                                    for (ink, needed) in [
                                        ("text", READABLE),
                                        ("secondary", check::art::HINT),
                                        ("muted", check::art::HINT),
                                    ] {
                                        let reads = contrast(get(ink), lit);
                                        assert!(
                                            reads >= needed,
                                            "{ink} on {wash} over the art: {reads:.2} ({spec:?})"
                                        );
                                    }
                                }
                            }
                        }
                        // And the theme still installs.
                        let stored = (background == Background::Picture).then_some(&picture.1[..]);
                        assert_eq!(refused(&spec, stored), None, "{spec:?}");
                    }
                }
            }
        }
    }
    assert!(tried >= 700);
}
