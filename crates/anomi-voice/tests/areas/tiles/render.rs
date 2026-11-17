//! The goldens' tiles drawn where the goldens have them, on the stage's
//! ground in a window of the goldens' size: `voice/stage.png`,
//! `voice/speaking_ring.png`, `voice/watching_stream.png` (the pointer on
//! the large tile) and `voice/with_camera.png`, and in Latte
//! `light/voice_stage.png`; and on a sheet of its own, `states`, those the
//! goldens don't show, the pointer on the third. A tile draws only where
//! the stage (V04) places
//! it, so until the stage's shots show them these stand in: with
//! `ANOMI_TILES_OUT` naming a folder, each is written there as a PNG at
//! `iced_test`'s scale, 2x, in the code's English as tests choose no
//! language. Without it they are drawn and dropped, so a tile that can't
//! draw fails here.

use iced::widget::{Stack, container};
use iced::{Background, Event, Fill, Padding, Point, Size, mouse};
use iced_test::Simulator;

use anomi_theme::prelude::*;
use anomi_vm::fixtures::voice::tiles as samples;
use anomi_vm::voice::tiles::{Intent, Tile};
use anomi_voice::tiles;

use super::super::support::{front, settings};

/// The goldens' window.
const WINDOW: Size = Size::new(1400.0, 860.0);

/// A golden's tiles and where each lies: left, top, width, height.
struct Golden {
    name: &'static str,
    theme: &'static str,
    tiles: fn() -> Vec<Tile>,
    places: &'static [(f32, f32, f32, f32)],
    pointer: Option<Point>,
}

/// Four in two rows of 420 by 236.25, 12 apart.
const GRID: &[(f32, f32, f32, f32)] = &[
    (462.0, 190.0, 420.0, 236.25),
    (894.0, 190.0, 420.0, 236.25),
    (462.0, 438.25, 420.0, 236.25),
    (894.0, 438.25, 420.0, 236.25),
];
/// Three in a row: in Latte, Bob gone.
const THREE: &[(f32, f32, f32, f32)] = &[
    (462.0, 190.0, 420.0, 236.25),
    (894.0, 190.0, 420.0, 236.25),
    (678.0, 438.25, 420.0, 236.25),
];
/// The large one, and the rest 192 by 108 beneath it.
const LARGE: (f32, f32, f32, f32) = (420.0, 109.0, 936.0, 526.5);
const WATCHING: &[(f32, f32, f32, f32)] = &[
    (588.0, 647.5, 192.0, 108.0),
    LARGE,
    (792.0, 647.5, 192.0, 108.0),
    (996.0, 647.5, 192.0, 108.0),
];
const CAMERA: &[(f32, f32, f32, f32)] = &[
    LARGE,
    (486.0, 647.5, 192.0, 108.0),
    (690.0, 647.5, 192.0, 108.0),
    (894.0, 647.5, 192.0, 108.0),
    (1098.0, 647.5, 192.0, 108.0),
];

/// Three rows of three, 420 by 236.25, 12 apart.
const SHEET: &[(f32, f32, f32, f32)] = &[
    (24.0, 24.0, 420.0, 236.25),
    (456.0, 24.0, 420.0, 236.25),
    (888.0, 24.0, 420.0, 236.25),
    (24.0, 272.25, 420.0, 236.25),
    (456.0, 272.25, 420.0, 236.25),
    (888.0, 272.25, 420.0, 236.25),
    (24.0, 520.5, 420.0, 236.25),
    (456.0, 520.5, 420.0, 236.25),
    (888.0, 520.5, 420.0, 236.25),
];

/// Your screen live and starting, a stream on its way, a stream offered
/// without and with its preview, your camera starting, Erin joining,
/// Carol's connection lost, and Bob's stream full screen.
fn states() -> Vec<Tile> {
    let mut full = samples::with_camera().swap_remove(0);
    full.full = true;
    let mut tiles = vec![
        samples::sharing(),
        samples::sharing_starting(),
        samples::waiting(),
        samples::offered(false),
        samples::offered(true),
        samples::starting(),
    ];
    tiles.extend(samples::joining());
    tiles.push(full);
    tiles
}

fn goldens() -> [Golden; 6] {
    let golden = |name, tiles, places| Golden {
        name,
        theme: "bonfire",
        tiles,
        places,
        pointer: None,
    };
    [
        golden("stage", samples::lounge, GRID),
        golden("speaking_ring", samples::speaking, GRID),
        Golden {
            pointer: Some(Point::new(1265.0, 220.0)),
            ..golden("watching_stream", samples::watching, WATCHING)
        },
        golden("with_camera", samples::with_camera, CAMERA),
        Golden {
            theme: "catppuccin-latte",
            ..golden("voice_stage_light", samples::quiet, THREE)
        },
        Golden {
            pointer: Some(Point::new(1100.0, 200.0)),
            ..golden("states", states, SHEET)
        },
    ]
}

/// The tiles at their places over the stage's ground.
fn placed<'a>(tiles: &'a [Tile], places: &[(f32, f32, f32, f32)]) -> Element<'a, Intent> {
    let ground = container(iced::widget::Space::new())
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().stage)),
            ..container::Style::default()
        });
    let mut layers = Stack::new().width(Fill).height(Fill).push(ground.boxed());
    for (tile, &(x, y, width, height)) in tiles.iter().zip(places) {
        let tile = container(tiles::view(tile, front()))
            .width(width)
            .height(height);
        layers = layers.push(
            container(tile)
                .padding(Padding::ZERO.left(x).top(y))
                .boxed(),
        );
    }
    layers.boxed()
}

#[test]
fn the_goldens_tiles_draw_where_the_goldens_have_them() {
    let out = std::env::var_os("ANOMI_TILES_OUT").map(std::path::PathBuf::from);
    for golden in goldens() {
        let tiles = (golden.tiles)();
        assert_eq!(tiles.len(), golden.places.len(), "{}", golden.name);
        let mut ui = Simulator::with_size(settings(), WINDOW, placed(&tiles, golden.places));
        if let Some(at) = golden.pointer {
            ui.point_at(at);
            ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
        }
        let theme = Theme::built_in(Some(golden.theme));
        let shot = ui.snapshot(&theme).expect("a snapshot");
        if let Some(out) = &out {
            // Written only where none is: `<name>-<renderer>.png`.
            let before = std::fs::read_dir(out).into_iter().flatten().flatten();
            for file in before.filter(|file| {
                let name = file.file_name();
                name.to_string_lossy()
                    .starts_with(&format!("{}-", golden.name))
            }) {
                let _ = std::fs::remove_file(file.path());
            }
            shot.matches_image(out.join(golden.name)).expect("written");
        }
    }
}
