//! Helpers to create a Youtube thumbnail images
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021 Michael Kefeder
use image::Rgba;
use image::imageops::overlay;
use imageproc::definitions::Clamp;
use imageproc::drawing::{Canvas, draw_text_mut, text_size};

/// Draws text centered to the image
fn draw_centered_text<I>(image: &mut I, color: I::Pixel, text: &str)
where
    I: Canvas,
    <I::Pixel as image::Pixel>::Subpixel: Into<f32> + Clamp<f32>,
{
    // Load the font
    // TODO: still true? ATTENTION Inter-VariableFont_slnt does not work, ttf parser unwrap() panics!
    let font_data = include_bytes!("../assets/Inter-Bold.ttf");
    // TODO: still true? - This only succeeds if collection consists of one font
    let font = ab_glyph::FontRef::try_from_slice(font_data).unwrap();

    let font_size = 192.0;
    let mut y_offset = 660;
    for text in text.split('\n') {
        // TODO: if too wide, auto-wrap text? inspiration <https://github.com/alexheretic/ab-glyph/blob/main/dev/src/layout.rs>
        let (glyphs_width, glyphs_height) = text_size(font_size, &font, text);
        assert!(glyphs_width < 1600);
        assert!(y_offset + glyphs_height < image.height());
        draw_text_mut(
            image,
            color,
            ((image.width() - glyphs_width) / 2).try_into().unwrap(),
            y_offset.try_into().unwrap(),
            font_size,
            &font,
            text,
        );
        y_offset += glyphs_height;
    }
}

pub fn make_thumbnail<P>(target: &P, background: &P, logos: &P, text: &str)
where
    P: AsRef<std::path::Path>,
{
    let sb_img = image::open(&background).expect("Can't open background image.");

    let mut image = sb_img.to_rgba8();

    let logos = image::open(&logos).expect("Can't open logos image.");
    let logos = logos.to_rgba8();
    draw_centered_text(&mut image, Rgba([227u8, 228u8, 229u8, 255u8]), text);
    overlay(&mut image, &logos, 0, 0);

    image.save(&target).unwrap();
}
