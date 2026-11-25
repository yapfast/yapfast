//! The colour that stands out most in a picture, which a profile without a
//! banner, accent or theme tints its banner with, as Discord does. No
//! toolkit in it: the window gives it pictures in either byte order.

/// The order of a pixel's bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Rgba,
    /// tiny-skia's, which the window keeps pictures in on the CPU.
    Bgra,
}

/// The colour that stands out most in `pixels`, four bytes each, as
/// 0xRRGGBB: the average of its opaque pixels, each counted by how
/// saturated it is, so a vivid avatar on a grey background gives its vivid
/// colour. A grey picture gives its plain average. None for a picture with
/// no opaque pixel.
pub fn dominant(pixels: &[u8], order: Order) -> Option<u32> {
    // About 1000 samples are enough for an average.
    let step = (pixels.len() / 4 / 1024).max(1);
    let (mut total, mut sum, mut count) = (0.0f64, [0.0f64; 3], 0.0f64);
    for pixel in pixels.as_chunks::<4>().0.iter().step_by(step) {
        if pixel[3] < 128 {
            continue;
        }
        let [r, g, b] = match order {
            Order::Rgba => [pixel[0], pixel[1], pixel[2]],
            Order::Bgra => [pixel[2], pixel[1], pixel[0]],
        };
        let channels = [f64::from(r), f64::from(g), f64::from(b)];
        let high = channels.iter().copied().fold(0.0, f64::max);
        let low = channels.iter().copied().fold(255.0, f64::min);
        // Saturation, with a floor so that grey pictures still average.
        let weight = (high - low) / 255.0 + 0.02;
        for (at, channel) in channels.iter().enumerate() {
            sum[at] += channel * weight;
        }
        total += weight;
        count += 1.0;
    }
    if count == 0.0 {
        return None;
    }
    let mut color = 0u32;
    for sum in sum {
        color = (color << 8) | (sum / total).round().clamp(0.0, 255.0) as u32;
    }
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vivid_colour_on_grey_wins_in_either_order() {
        let mut rgba = Vec::new();
        for at in 0..1024 {
            rgba.extend(if at % 8 == 0 {
                [200, 20, 20, 255]
            } else {
                [120, 120, 120, 255]
            });
        }
        let color = dominant(&rgba, Order::Rgba).unwrap();
        let (red, green) = ((color >> 16) & 0xff, (color >> 8) & 0xff);
        assert!(red > 150 && green < 90, "{color:06x}");
        let bgra: Vec<u8> = rgba
            .chunks(4)
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
            .collect();
        assert_eq!(dominant(&bgra, Order::Bgra), Some(color));
        assert_eq!(dominant(&[0; 64], Order::Rgba), None, "see-through");
    }
}
