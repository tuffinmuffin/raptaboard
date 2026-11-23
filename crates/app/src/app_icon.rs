//! raptaboard's own icon: the window's, and the files the release packages
//! use (a Windows `.ico`, PNGs for the macOS `.icns` and Linux).
//!
//! It's drawn from one SVG, `media/icon/raptaboard.svg`: Raptacon's raptor
//! mark (from Raptacon's brand kit) in Raptacon Green inside a rust-red
//! gear. The window draws it when the program starts; the files in
//! `media/icon/` are made from it with
//! `cargo run -p raptaboard --example make_icons`.

use std::io::Cursor;

use eframe::egui;

/// The icon's SVG.
pub const SVG: &[u8] = include_bytes!("../../../media/icon/raptaboard.svg");

/// The sizes, in pixels, put in the Windows `.ico` (Explorer and the
/// taskbar pick the one that fits).
pub const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// The Microsoft Store package's logos (`media/icon/msix/`), by file name
/// and size in pixels: the Store's listing, the Start menu tile, and the
/// taskbar and app list.
pub const MSIX_LOGOS: [(&str, u32); 3] = [
    ("StoreLogo.png", 50),
    ("Square150x150Logo.png", 150),
    ("Square44x44Logo.png", 44),
];

/// The icon drawn `size` pixels square, four bytes a pixel (red, green,
/// blue, alpha), row by row.
///
/// ```
/// let pixels = raptaboard::app_icon::rgba(32);
/// assert_eq!(pixels.len(), 32 * 32 * 4);
/// ```
pub fn rgba(size: u32) -> Vec<u8> {
    let drawn = crate::images::decode(SVG, "raptaboard.svg", size).expect("the icon's SVG reads");
    debug_assert_eq!((drawn.width, drawn.height), (size, size));
    drawn.pixels
}

/// The window's icon (256 pixels; the system scales it).
pub fn window_icon() -> egui::IconData {
    let size = 256;
    egui::IconData {
        rgba: rgba(size),
        width: size,
        height: size,
    }
}

/// The icon as a PNG file, `size` pixels square.
///
/// ```
/// let png = raptaboard::app_icon::png(64);
/// assert_eq!(&png[1..4], b"PNG");
/// ```
pub fn png(size: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_raw(size, size, rgba(size)).expect("size matches");
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("PNG writes to memory");
    out.into_inner()
}

/// The icon as a Windows `.ico` file holding a PNG at each of `sizes`
/// (each 1 to 256 pixels). The format: a 6-byte header, a 16-byte entry
/// per image (its size, where its bytes are), then the PNGs.
///
/// ```
/// let ico = raptaboard::app_icon::ico(&[16, 32]);
/// assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]); // an icon file, 2 images
/// ```
pub fn ico(sizes: &[u32]) -> Vec<u8> {
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect();
    let count = u16::try_from(sizes.len()).expect("a few sizes");
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for (&size, png) in sizes.iter().zip(&pngs) {
        assert!((1..=256).contains(&size), "ico sizes are 1 to 256");
        // 256 is written as 0: the field is one byte.
        let side = if size == 256 { 0 } else { size as u8 };
        let len = u32::try_from(png.len()).expect("small PNG");
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += len;
    }
    for png in pngs {
        out.extend_from_slice(&png);
    }
    out
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn the_icon_is_a_gear_around_a_dark_disc_with_see_through_corners() {
        let size = 256;
        let pixels = rgba(size);
        let rust = [0xB7, 0x41, 0x0E, 255];
        assert_eq!(pixel(&pixels, size, 2, 2)[3], 0, "corner see-through");
        // Between two teeth on the left (the gap at 180 degrees) is see-through;
        // the top tooth is the gear's rust red.
        assert_eq!(pixel(&pixels, size, 12, 128)[3], 0, "between teeth");
        assert_eq!(pixel(&pixels, size, 128, 10), rust, "top tooth");
        assert_eq!(pixel(&pixels, size, 128, 30), rust, "gear rim");
        // The dark disc below the raptor.
        assert_eq!(pixel(&pixels, size, 128, 205), [0x0F, 0x17, 0x20, 255]);
    }

    #[test]
    fn the_ico_lists_each_png_where_it_is() {
        let sizes = [16, 256];
        let ico = ico(&sizes);
        let u32_at = |i: usize| u32::from_le_bytes(ico[i..i + 4].try_into().unwrap());
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
        let mut expected_offset = 6 + 16 * 2;
        for (n, &size) in sizes.iter().enumerate() {
            let entry = 6 + 16 * n;
            let side = if size == 256 { 0 } else { size as u8 };
            assert_eq!(ico[entry], side, "width");
            assert_eq!(ico[entry + 1], side, "height");
            let (len, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
            assert_eq!(offset, expected_offset);
            let png = &ico[offset..offset + len];
            assert_eq!(png, super::png(size).as_slice());
            expected_offset += len;
        }
        assert_eq!(ico.len(), expected_offset, "nothing after the last PNG");
    }
}
