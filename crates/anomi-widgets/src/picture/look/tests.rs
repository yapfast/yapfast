//! Copies at a size and in a shape: averages, cuts and corners.

use super::*;

fn flat(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel.repeat((width * height) as usize)
}

fn at(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = (y * width + x) as usize * 4;
    rgba[at..at + 4].try_into().unwrap()
}

fn look(width: u32, height: u32, radius: f32) -> Look {
    Look {
        width,
        height,
        corners: [(radius * 4.0) as u16; 4],
        cover: false,
    }
}

#[test]
fn a_round_copy_is_clear_at_its_corners_and_whole_inside() {
    let source = flat(128, 128, [200, 100, 50, 255]);
    let copy = make(&source, 128, 128, &look(32, 32, 16.0));
    assert_eq!(copy.len(), 32 * 32 * 4);
    for (x, y) in [(0, 0), (31, 0), (31, 31), (0, 31)] {
        assert_eq!(at(&copy, 32, x, y)[3], 0, "corner {x},{y}");
    }
    for (x, y) in [(16, 16), (8, 20), (24, 12)] {
        assert_eq!(at(&copy, 32, x, y), [200, 100, 50, 255], "inside {x},{y}");
    }
    // The middles of the sides are nearly whole.
    for (x, y) in [(16, 0), (0, 16), (31, 16), (16, 31)] {
        assert!(at(&copy, 32, x, y)[3] > 250, "side {x},{y}");
    }
    // The edge between is soft.
    let edge = at(&copy, 32, 4, 4)[3];
    assert!(edge > 0 && edge < 255, "{edge}");
}

#[test]
fn shrinking_averages_what_each_pixel_covers() {
    // Black and white columns, two pixels wide.
    let mut source = Vec::new();
    for _ in 0..4 {
        for x in 0..4 {
            let value = if x % 2 == 0 { 0 } else { 255 };
            source.extend([value, value, value, 255]);
        }
    }
    let copy = make(&source, 4, 4, &look(2, 2, 0.0));
    for pixel in copy.chunks(4) {
        assert!((127..=128).contains(&pixel[0]), "{pixel:?}");
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn a_clear_pixel_lends_no_colour() {
    // Clear black beside opaque white: the average is white, half there.
    let source = [0, 0, 0, 0, 255, 255, 255, 255];
    let copy = make(&source, 2, 1, &look(1, 1, 0.0));
    assert_eq!(copy, [255, 255, 255, 128]);
}

#[test]
fn growing_keeps_a_flat_colour_flat() {
    let source = flat(2, 2, [10, 20, 30, 255]);
    let copy = make(&source, 2, 2, &look(5, 5, 0.0));
    assert!(copy.chunks(4).all(|pixel| pixel == [10, 20, 30, 255]));
}

#[test]
fn covering_takes_the_middle() {
    // Red, green and blue bands two pixels wide; a square takes green.
    let mut source = Vec::new();
    for _ in 0..2 {
        for x in 0..6 {
            source.extend(match x / 2 {
                0 => [255, 0, 0, 255],
                1 => [0, 255, 0, 255],
                _ => [0, 0, 255, 255],
            });
        }
    }
    let cut = Look {
        cover: true,
        ..look(2, 2, 0.0)
    };
    let copy = make(&source, 6, 2, &cut);
    assert!(
        copy.chunks(4).all(|pixel| pixel == [0, 255, 0, 255]),
        "{copy:?}"
    );
}

#[test]
fn the_same_size_is_copied_as_it_is() {
    let source: Vec<u8> = (0..64).collect();
    assert_eq!(make(&source, 4, 4, &look(4, 4, 0.0)), source);
}

#[test]
fn a_short_picture_comes_out_clear() {
    assert_eq!(make(&[1, 2, 3], 4, 4, &look(2, 1, 0.0)), [0; 8]);
}

#[test]
fn a_ten_pixel_blur_drawn_large_keeps_its_corners() {
    // A spoiler's blur, 10 by 7, drawn 200 by 125 with corners of 8.
    let source = flat(10, 7, [200, 100, 50, 255]);
    let copy = make(&source, 10, 7, &look(200, 125, 8.0));
    assert_eq!(copy.len(), 200 * 125 * 4);
    for (x, y) in [(0, 0), (199, 0), (199, 124), (0, 124)] {
        assert_eq!(at(&copy, 200, x, y)[3], 0, "corner {x},{y}");
    }
    assert_eq!(at(&copy, 200, 100, 62), [200, 100, 50, 255]);
    // Eight pixels in, the edge is whole again.
    assert_eq!(at(&copy, 200, 8, 0)[3], 255);
}

/// What this process has resident, or most since its peak was reset, in
/// KiB; none where `/proc` doesn't say.
fn resident(key: &str) -> Option<usize> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// A 4000 by 3000 photo shrunk to 1000 by 750, and a preview grown to twice
/// its size with rounded corners, each hold little more than the copy while
/// it's made: a few of the picture's rows, not all of them. Measured in a
/// process of its own, where no other test's memory counts.
#[test]
fn a_copy_holds_little_more_than_itself_while_made() {
    const ALONE: &str = "ANOMI_PICTURE_COPY_ALONE";
    const NAME: &str = "a_copy_holds_little_more_than_itself_while_made";
    if std::env::var_os(ALONE).is_none() {
        let path = module_path!().split_once("::").map_or("", |(_, path)| path);
        let run = std::process::Command::new(std::env::current_exe().expect("the tests"))
            .args([&format!("{path}::{NAME}"), "--exact", "--test-threads=1"])
            .env(ALONE, "1")
            .output()
            .expect("the tests run again");
        let said = String::from_utf8_lossy(&run.stdout);
        assert!(run.status.success(), "{said}");
        assert!(said.contains("1 passed"), "{said}");
        return;
    }
    let cases = [
        (4000, 3000, look(1000, 750, 0.0)),
        (550, 400, look(1100, 800, 8.0)),
    ];
    // Small ones first, so the pages of the tests' code it runs are in and
    // don't count.
    for (width, height, look) in cases {
        let (small, few) = (width / 50, height / 50);
        let look = Look {
            width: look.width / 50,
            height: look.height / 50,
            ..look
        };
        make(&flat(small, few, [9; 4]), small, few, &look);
    }
    for (width, height, look) in cases {
        let source: Vec<u8> = (0..width * height * 4).map(|at| (at % 251) as u8).collect();
        let Some(before) = resident("VmRSS:") else {
            return;
        };
        if std::fs::write("/proc/self/clear_refs", "5").is_err() {
            return;
        }
        let copy = make(&source, width, height, &look);
        let held = resident("VmHWM:").unwrap_or(before).saturating_sub(before) * 1024;
        assert!(
            held <= copy.len() + (1 << 20),
            "{width}x{height} to {}x{}: {held} bytes held for a copy of {}",
            look.width,
            look.height,
            copy.len()
        );
    }
}

#[test]
fn only_a_change_needs_a_copy() {
    assert!(!look(128, 128, 0.0).needs_copy(128, 128));
    assert!(look(64, 64, 0.0).needs_copy(128, 128), "a mip level below");
    assert!(
        look(32, 32, 0.0).needs_copy(128, 128),
        "four times the side"
    );
    assert!(look(128, 128, 2.0).needs_copy(128, 128), "corners");
    let wide = Look {
        cover: true,
        ..look(300, 100, 0.0)
    };
    assert!(wide.needs_copy(200, 200), "a shape to cut");
    assert!(
        !wide.needs_copy(390, 130),
        "already that shape, and near its size"
    );
}
