use super::*;

/// A `text/uri-list` is X11's and Wayland's, so a path in one is written
/// as they offer it, as a URI; the desktop's own clipboard names files
/// another way.
#[cfg(target_os = "linux")]
#[test]
fn reads_local_files_from_a_uri_list() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a map #1.txt");
    std::fs::write(&file, "map").unwrap();
    let encoded = file
        .to_str()
        .unwrap()
        .replace(' ', "%20")
        .replace('#', "%23");
    let list = format!(
        "# copied\r\nfile://{encoded}\r\nfile://localhost{encoded}\r\nhttps://example.com/a.png\r\nfile:///no/such/file\r\n"
    );
    assert_eq!(files(&list), vec![file.clone(), file]);
}

#[test]
fn reads_no_percent_escape_it_cannot_decode() {
    assert_eq!(percent_decode("%zz"), None);
    assert_eq!(percent_decode("%20"), Some(" ".into()));
}

/// A `BITMAPV5HEADER` for a 2×2 bitmap, with these masks.
fn v5_header(bits: u16, compression: u32, masks: [u32; 4], height: i32) -> Vec<u8> {
    let mut header = vec![0u8; 124];
    header[0..4].copy_from_slice(&124u32.to_le_bytes());
    header[4..8].copy_from_slice(&2i32.to_le_bytes());
    header[8..12].copy_from_slice(&height.to_le_bytes());
    header[12..14].copy_from_slice(&1u16.to_le_bytes());
    header[14..16].copy_from_slice(&bits.to_le_bytes());
    header[16..20].copy_from_slice(&compression.to_le_bytes());
    for (at, mask) in (40..).step_by(4).zip(masks) {
        header[at..at + 4].copy_from_slice(&mask.to_le_bytes());
    }
    header
}

/// Two rows of two BGRA pixels, the bottom row first.
const BGRA: [u8; 16] = [
    0, 0, 255, 128, 0, 255, 0, 255, // bottom: red at half alpha, green
    255, 0, 0, 255, 255, 255, 255, 0, // top: blue, white fully transparent
];
const RGBA: [u8; 16] = [
    0, 0, 255, 255, 255, 255, 255, 0, // top
    255, 0, 0, 128, 0, 255, 0, 255, // bottom
];

#[test]
fn reads_alpha_from_a_32_bit_bitmap_without_masks() {
    // As Windows makes CF_DIBV5 of Chromium's CF_DIB: BI_RGB, with an
    // alpha mask, the size exact. arboard 3.6.1 fails on this one.
    let mut dib = v5_header(32, 0, [0, 0, 0, 0xff00_0000], 2);
    dib.extend(BGRA);
    assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
    // Rounded up past the data, as GlobalSize may report it.
    dib.extend([0; 16]);
    assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);

    // A 40-byte header from the top down.
    let mut dib = v5_header(32, 0, [0; 4], -2);
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib.truncate(40);
    dib.extend(BGRA[8..].iter().chain(&BGRA[..8]));
    assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
}

#[test]
fn reads_a_bitmap_with_masks_after_the_header() {
    // Snipping Tool's: BI_BITFIELDS, its masks in the header and again
    // after it.
    let masks = [0xff_0000, 0xff00, 0xff, 0xff00_0000];
    let mut dib = v5_header(32, 3, masks, 2);
    dib.extend(masks[..3].iter().flat_map(|mask| mask.to_le_bytes()));
    dib.extend(BGRA);
    assert_eq!(bitmap(&dib).unwrap().into_raw(), RGBA);
}

#[test]
fn a_bitmap_without_alpha_is_opaque() {
    let mut opaque = BGRA;
    opaque
        .iter_mut()
        .skip(3)
        .step_by(4)
        .for_each(|alpha| *alpha = 0);
    let mut dib = v5_header(32, 0, [0; 4], 2);
    dib.extend(opaque);
    let pixels = bitmap(&dib).unwrap();
    assert!(pixels.pixels().all(|pixel| pixel[3] == 255));

    // 24-bit, rows padded to four bytes.
    let mut dib = v5_header(24, 0, [0; 4], 1);
    dib.extend([0, 0, 255, 0, 255, 0, 0, 0]);
    assert_eq!(
        bitmap(&dib).unwrap().into_raw(),
        [255, 0, 0, 255, 0, 255, 0, 255]
    );
}

#[test]
fn refuses_a_short_bitmap() {
    let mut dib = v5_header(32, 0, [0; 4], 2);
    dib.extend(&BGRA[..12]);
    assert!(bitmap(&dib).is_none());
    assert!(bitmap(&dib[..20]).is_none());
}

/// Reads the desktop's clipboard, for checking by hand: copy files or a
/// picture, then run with `--ignored --nocapture`.
#[test]
#[ignore = "reads the desktop clipboard"]
fn reads_the_desktop_clipboard() {
    #[cfg(target_os = "linux")]
    if let Err(error) = wayland::read() {
        println!("wayland error {error:?}");
    }
    match read() {
        Clipboard::Files(files) => println!("files {files:?}"),
        Clipboard::Picture(bytes, extension) => println!("picture {extension} {}", bytes.len()),
        Clipboard::Other => println!("other"),
    }
}
