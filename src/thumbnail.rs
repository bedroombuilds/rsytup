//! Helpers to create YouTube thumbnail images
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use std::path::{Path, PathBuf};

use exn::{Result, ResultExt, bail};
use image::Rgba;
use image::imageops::overlay;
use imageproc::definitions::Clamp;
use imageproc::drawing::{Canvas, draw_text_mut, text_size};
use thiserror::Error;

/// Maximum width of a line of text on the thumbnail, in pixels.
const MAX_TEXT_WIDTH: u32 = 1600;

/// What went wrong, categorized by what a caller *can do* about it:
/// none of these kinds are retryable, but they point to different fixes.
#[derive(Debug)]
pub enum ThumbnailErrorKind {
    /// The background image could not be opened or decoded.
    OpenBackground(PathBuf),
    /// The logos image could not be opened or decoded.
    OpenLogos(PathBuf),
    /// A line of the text does not fit on the image. Shorten the title or
    /// break it into more lines.
    TextTooLarge(String),
    /// The thumbnail could not be written to the target path.
    Save,
}

impl ThumbnailErrorKind {
    /// A short, human-readable label for the failure.
    fn label(&self) -> String {
        match self {
            Self::OpenBackground(path) => format!("could not open background {}", path.display()),
            Self::OpenLogos(path) => format!("could not open logos {}", path.display()),
            Self::TextTooLarge(line) => format!("text line `{line}` does not fit on the image"),
            Self::Save => "could not save the image".to_string(),
        }
    }
}

/// A failure while rendering a thumbnail image.
#[derive(Debug, Error)]
#[error("failed to make thumbnail {}: {}", self.target.display(), self.kind.label())]
pub struct ThumbnailError {
    kind: ThumbnailErrorKind,
    /// The thumbnail file that was supposed to be written.
    target: PathBuf,
}

/// Draws text centered to the image; returns the first line that does not fit.
fn draw_centered_text<'a, I>(
    image: &mut I,
    color: I::Pixel,
    text: &'a str,
) -> std::result::Result<(), &'a str>
where
    I: Canvas,
    <I::Pixel as image::Pixel>::Subpixel: Into<f32> + Clamp<f32>,
{
    // Load the font
    // TODO: still true? ATTENTION Inter-VariableFont_slnt does not work, ttf parser unwrap() panics!
    let font_data = include_bytes!("../assets/Inter-Bold.ttf");
    // TODO: still true? - This only succeeds if collection consists of one font
    let font = ab_glyph::FontRef::try_from_slice(font_data).expect("embedded font is valid");

    let font_size = 192.0;
    let mut y_offset = 660;
    for text in text.split('\n') {
        // TODO: if too wide, auto-wrap text? inspiration <https://github.com/alexheretic/ab-glyph/blob/main/dev/src/layout.rs>
        let (glyphs_width, glyphs_height) = text_size(font_size, &font, text);
        if glyphs_width >= MAX_TEXT_WIDTH.min(image.width())
            || y_offset + glyphs_height >= image.height()
        {
            return Err(text);
        }
        draw_text_mut(
            image,
            color,
            ((image.width() - glyphs_width) / 2)
                .try_into()
                .expect("image dimensions fit in i32"),
            y_offset.try_into().expect("image dimensions fit in i32"),
            font_size,
            &font,
            text,
        );
        y_offset += glyphs_height;
    }
    Ok(())
}

pub fn make_thumbnail<P>(
    target: &P,
    background: &P,
    logos: &P,
    text: &str,
) -> Result<(), ThumbnailError>
where
    P: AsRef<Path>,
{
    let error = |kind| ThumbnailError {
        kind,
        target: target.as_ref().to_path_buf(),
    };
    let sb_img = image::open(background).or_raise(|| {
        error(ThumbnailErrorKind::OpenBackground(
            background.as_ref().to_path_buf(),
        ))
    })?;

    let mut image = sb_img.to_rgba8();

    let logos = image::open(logos)
        .or_raise(|| error(ThumbnailErrorKind::OpenLogos(logos.as_ref().to_path_buf())))?;
    let logos = logos.to_rgba8();
    if let Err(line) = draw_centered_text(&mut image, Rgba([227u8, 228u8, 229u8, 255u8]), text) {
        bail!(error(ThumbnailErrorKind::TextTooLarge(line.to_string())));
    }
    overlay(&mut image, &logos, 0, 0);

    let image = image::DynamicImage::ImageRgba8(image);
    image
        .to_rgb8()
        .save(target)
        .or_raise(|| error(ThumbnailErrorKind::Save))
}
