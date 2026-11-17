//! What each style draws, read off the pixels: the fake Discord's samples
//! (`nameStyleSample` in `testing/fake_discord.ts`) at 32 px on Bonfire.

use iced::Size;

use anomi_theme::Theme;

use super::super::{Ink, Style, styled_name};
use super::support::{inked, shot};

fn drawn(text: &str, style: Option<Style>) -> Vec<(u32, u32, [u8; 4])> {
    let name = styled_name(text, style).size(32.0);
    inked(&shot(
        name,
        Size::new(260.0, 60.0),
        &Theme::built_in(None),
        None,
    ))
}

/// The mean colour of `pixels`.
fn mean(pixels: &[(u32, u32, [u8; 4])]) -> [f32; 3] {
    let count = pixels.len().max(1) as f32;
    let mut sum = [0.0; 3];
    for (_, _, pixel) in pixels {
        for (total, channel) in sum.iter_mut().zip(pixel) {
            *total += f32::from(*channel);
        }
    }
    sum.map(|total| total / count)
}

/// The mean colours of the left, middle and right thirds of `pixels`.
fn thirds(pixels: &[(u32, u32, [u8; 4])]) -> [[f32; 3]; 3] {
    let (min, max) = pixels.iter().fold((u32::MAX, 0), |(min, max), (x, _, _)| {
        (min.min(*x), max.max(*x))
    });
    let third = (max - min) / 3;
    let part = |part: u32| {
        let from = min + third * part;
        let within: Vec<_> = pixels
            .iter()
            .copied()
            .filter(|(x, ..)| (from..from + third).contains(x))
            .collect();
        mean(&within)
    };
    [part(0), part(1), part(2)]
}

fn left_and_right(pixels: &[(u32, u32, [u8; 4])]) -> ([f32; 3], [f32; 3]) {
    let [left, _, right] = thirds(pixels);
    (left, right)
}

#[test]
fn a_gradient_runs_from_its_first_colour_to_its_last() {
    // Carol's: pink to violet, in Sakura.
    let pixels = drawn("Carol", Style::new(3, 2, &[0xff7eb3, 0x7a5cff]));
    assert!(pixels.len() > 500, "{} pixels", pixels.len());
    let (left, right) = left_and_right(&pixels);
    assert!(left[0] > left[2], "pink at the left: {left:?}");
    assert!(right[2] > right[0], "violet at the right: {right:?}");
}

#[test]
fn a_prism_comes_back_to_its_first_colour() {
    let pixels = drawn("mmmmmmmm", Style::new(11, 7, &[0xff0000, 0x0000ff]));
    let [left, middle, right] = thirds(&pixels);
    assert!(
        left[0] > left[2] && right[0] > right[2],
        "{left:?} {right:?}"
    );
    assert!(middle[2] > middle[0], "blue half way: {middle:?}");
}

#[test]
fn neon_is_white_edged_in_its_colour() {
    let pixels = drawn("neon", Style::new(14, 3, &[0x00ff99]));
    let white = pixels
        .iter()
        .filter(|(_, _, p)| p[0] > 230 && p[1] > 230 && p[2] > 230)
        .count();
    let green = pixels
        .iter()
        .filter(|(_, _, p)| i32::from(p[1]) > i32::from(p[0]) + 40)
        .count();
    assert!(white > 100 && green > 100, "{white} white, {green} green");
}

#[test]
fn a_faded_neon_name_is_faded_as_a_whole() {
    // Slint fades the name as one layer, as an offline member's: the edge
    // under the letters never shows through them, so a letter at half is
    // white over the ground, a neutral grey, and not green.
    let style = Style::new(14, 3, &[0x00ff99]);
    let theme = Theme::built_in(None);
    let size = Size::new(260.0, 60.0);
    let full = shot(styled_name("neon", style).size(32.0), size, &theme, None);
    let faded = styled_name("neon", style).size(32.0).opacity(0.5);
    let faded = shot(faded, size, &theme, None);
    let white = |x: u32, y: u32| full.get_pixel(x, y).0[..3].iter().all(|c| *c > 230);
    // Well inside a letter: white two pixels around at full opacity.
    let (width, height) = full.dimensions();
    let inside: Vec<_> = (2..width - 2)
        .flat_map(|x| (2..height - 2).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            white(x, y) && white(x - 2, y) && white(x + 2, y) && white(x, y - 2) && white(x, y + 2)
        })
        .map(|(x, y)| (x, y, faded.get_pixel(x, y).0))
        .collect();
    assert!(inside.len() > 50, "{} pixels inside", inside.len());
    let [r, g, b] = mean(&inside);
    assert!((g - r).abs() < 15.0, "neutral inside: {r} {g} {b}");
    let ground = faded.get_pixel(0, 0).0.map(f32::from);
    assert!(
        r > ground[0] + 40.0,
        "white at half: {r} over {}",
        ground[0]
    );
    // The edge outside the letters still shows, at half.
    let green = inked(&faded)
        .iter()
        .filter(|(_, _, p)| i32::from(p[1]) > i32::from(p[0]) + 20)
        .count();
    assert!(green > 100, "the edge: {green}");
}

#[test]
fn toon_is_edged_in_its_dark_shade_and_lighter_on_top() {
    let pixels = drawn("Toon", Style::new(11, 4, &[0xffaa33]));
    let dark = pixels
        .iter()
        .filter(|(_, _, p)| p[0] < 120 && p[0] > p[2] + 15)
        .count();
    assert!(dark > 50, "the edge: {dark}");
    let (top, bottom) = {
        let (min, max) = pixels.iter().fold((u32::MAX, 0), |(min, max), (_, y, _)| {
            (min.min(*y), max.max(*y))
        });
        let band = (max - min) / 4;
        let top: Vec<_> = pixels
            .iter()
            .copied()
            .filter(|(_, y, _)| *y < min + band + 4)
            .collect();
        let bottom: Vec<_> = pixels
            .iter()
            .copied()
            .filter(|(_, y, _)| *y > max - band - 4)
            .collect();
        (mean(&top), mean(&bottom))
    };
    let light = |colour: [f32; 3]| colour.iter().sum::<f32>();
    assert!(light(top) > light(bottom), "{top:?} over {bottom:?}");
}

#[test]
fn pop_is_white_over_a_copy_in_its_colour() {
    let pixels = drawn("Pop", Style::new(11, 5, &[0x3388ff]));
    let white = pixels
        .iter()
        .filter(|(_, _, p)| p[0] > 230 && p[1] > 230 && p[2] > 230)
        .count();
    let blue = pixels
        .iter()
        .filter(|(_, _, p)| i32::from(p[2]) > i32::from(p[0]) + 60)
        .count();
    assert!(white > 100 && blue > 50, "{white} white, {blue} blue");
    let lowest = |keep: &dyn Fn(&[u8; 4]) -> bool| {
        pixels
            .iter()
            .filter(|(_, _, p)| keep(p))
            .map(|(_, y, _)| *y)
            .max()
    };
    let white_low = lowest(&|p| p[0] > 230 && p[1] > 230);
    let blue_low = lowest(&|p| i32::from(p[2]) > i32::from(p[0]) + 60);
    assert!(
        blue_low > white_low,
        "the copy is lower: {blue_low:?} {white_low:?}"
    );
}

#[test]
fn a_gummy_name_puts_its_letters_in_turn() {
    let pixels = drawn("ab", Style::new(11, 8, &[0xff0000, 0x00ff00]));
    let (left, right) = left_and_right(&pixels);
    assert!(left[0] > left[1], "a is red: {left:?}");
    assert!(right[1] > right[0], "b is green: {right:?}");
}

#[test]
fn a_name_without_colours_is_in_its_ink() {
    let name = styled_name("Ann", None)
        .size(32.0)
        .ink(Ink::Fixed(iced::Color::from_rgb8(0xf0, 0xb2, 0x32)));
    let pixels = inked(&shot(
        name,
        Size::new(200.0, 60.0),
        &Theme::built_in(None),
        None,
    ));
    let gold = mean(&pixels);
    assert!(gold[0] > gold[2] + 60.0, "{gold:?}");
    // Halved, as an offline member's.
    let faded = styled_name("Ann", None).size(32.0).opacity(0.5);
    let full = styled_name("Ann", None).size(32.0);
    let theme = Theme::built_in(None);
    let size = Size::new(200.0, 60.0);
    let light = |pixels: Vec<(u32, u32, [u8; 4])>| mean(&pixels).iter().sum::<f32>();
    assert!(
        light(inked(&shot(faded, size, &theme, None)))
            < light(inked(&shot(full, size, &theme, None)))
    );
}
