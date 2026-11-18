//! What the frames took, printed.

use std::time::Duration;

use super::Totals;

/// The time a frame took by step, what was redrawn, and the busy
/// share of the run.
pub fn report(totals: &Totals, wall: f64) {
    let frames = totals.frames.max(1) as f64;
    let ms = |time: Duration| time.as_secs_f64() * 1e3 / frames;
    let busy = totals.events
        + totals.build
        + totals.view
        + totals.redraw
        + totals.layers
        + totals.diff
        + totals.draw;
    println!(
        "{} frames, {} views: per frame {:.3} ms = events {:.3} + view {:.3} + build {:.3} \
         + redraw {:.3} + layers {:.3} + diff {:.3} + draw {:.3}; {:.0} px redrawn; \
         busy {:.1}% of the {:.1} s",
        totals.frames,
        totals.views,
        ms(busy),
        ms(totals.events),
        ms(totals.view),
        ms(totals.build),
        ms(totals.redraw),
        ms(totals.layers),
        ms(totals.diff),
        ms(totals.draw),
        totals.area / frames,
        busy.as_secs_f64() / wall * 100.0,
        wall,
    );
}

/// The renderer's time by kind of primitive, a frame, and the pictures it
/// drew at another size than their own.
pub fn kinds(frames: usize) {
    let (kinds, scaled) = iced_tiny_skia::stats::take();
    let frames = frames.max(1) as f64;
    let line: Vec<String> = kinds
        .iter()
        .filter(|(_, _, count)| *count > 0)
        .map(|(kind, time, count)| {
            format!(
                "{kind:?} {:.3} ms ({:.0})",
                time.as_secs_f64() * 1e3 / frames,
                *count as f64 / frames
            )
        })
        .collect();
    println!("draw by kind, a frame: {}", line.join(", "));
    let mut sizes: Vec<_> = scaled.clone();
    sizes.sort_unstable();
    sizes.dedup();
    if !sizes.is_empty() {
        let shown: Vec<String> = sizes
            .iter()
            .take(12)
            .map(|((w, h), (dw, dh))| {
                let times = scaled
                    .iter()
                    .filter(|s| s.0 == (*w, *h) && s.1 == (*dw, *dh))
                    .count();
                format!("{w}x{h} at {dw}x{dh} ({times})")
            })
            .collect();
        println!("drawn at another size: {}", shown.join(", "));
    }
}
