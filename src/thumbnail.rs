//! Helpers to create a Youtube thumbnail images
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021 Michael Kefeder
use conv::ValueInto;
use image::imageops::overlay;
use image::Rgba;
use imageproc::definitions::Clamp;
use imageproc::drawing::{draw_text_mut, Canvas};
use rusttype::{point, Font, Scale};

/// Draws text centered to the image
fn draw_centered_text<I>(image: &mut I, color: I::Pixel, text: &str)
where
    I: Canvas,
    <I::Pixel as image::Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
{
    // Load the font
    // ATTENTION Inter-VariableFont_slnt does not work, ttf parser unwrap() panics!
    let font_data = include_bytes!("../assets/Inter-Bold.ttf");
    // This only succeeds if collection consists of one font
    let font = Font::try_from_bytes(font_data as &[u8]).expect("Error constructing Font");

    // The font size to use
    let scale = Scale::uniform(192.0);

    let v_metrics = font.v_metrics(scale);

    let mut y_offset = 660;
    for text in text.split('\n') {
        // layout the glyphs in a line with 20 pixels padding
        let glyphs: Vec<_> = font
            .layout(text, scale, point(20.0, 20.0 + v_metrics.ascent))
            .collect();
        // work out the layout size
        let glyphs_height = (v_metrics.ascent - v_metrics.descent).ceil() as u32;
        let glyphs_width = {
            let min_x = glyphs
                .first()
                .map(|g| g.pixel_bounding_box().unwrap().min.x)
                .unwrap();
            let max_x = glyphs
                .last()
                .map(|g| g.pixel_bounding_box().unwrap().max.x)
                .unwrap();
            (max_x - min_x) as u32
        };
        // TODO: if too wide, auto-wrap text?
        assert!(glyphs_width < 1600);
        assert!(y_offset + glyphs_height < image.height());
        draw_text_mut(
            image,
            color,
            ((image.width() - glyphs_width) / 2).try_into().unwrap(),
            y_offset.try_into().unwrap(),
            scale,
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

    let _ = image.save(&target).unwrap();
}
