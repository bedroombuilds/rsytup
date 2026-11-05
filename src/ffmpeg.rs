//! ffmpeg helper functions
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use exn::{Result, ResultExt, bail, ensure};

/// What went wrong, categorized by what a caller *can do* about it:
/// none of these kinds are retryable, but they point to different fixes.
#[derive(Debug)]
pub enum FfmpegErrorKind {
    /// The `ffmpeg` binary could not be started (e.g. it does not exist at the
    /// configured path).
    SpawnFailed,
    /// `ffmpeg` ran but exited with a non-zero status. Its stderr was already
    /// forwarded to the terminal, so the underlying cause is visible there.
    Exited(i32),
    /// `ffmpeg` ran, but its output could not be forwarded to our stdout /
    /// stderr (e.g. a closed pipe). The screenshot may still have been written.
    ForwardOutput,
    /// `ffmpeg` exited successfully but no screenshot file was written.
    ///
    /// This is the "Output file is empty, nothing was encoded" case, where
    /// `ffmpeg` reports *success* despite producing nothing; we detect it by
    /// verifying that the screenshot file exists after the run.
    NoOutput,
}

/// A failure while making a screenshot of a video with `ffmpeg`.
///
/// Modeled as a flat, kind-based error.
/// [`FfmpegErrorKind`] says what the caller can do about the failure, while
/// the fields carry the context a human needs to debug it.
#[derive(Debug, Error)]
#[error(
    "failed to make screenshot {} of video {} at second {}: {}",
    self.screenshot.display(),
    self.video.display(),
    self.at_second,
    self.kind.label()
)]
pub struct FfmpegError {
    kind: FfmpegErrorKind,
    /// The video the screenshot was requested for.
    video: PathBuf,
    /// The screenshot file that was supposed to be written.
    screenshot: PathBuf,
    /// The video second the screenshot was requested at.
    at_second: usize,
}

impl FfmpegError {
    fn new(kind: FfmpegErrorKind, video: &Path, screenshot: &Path, at_second: usize) -> Self {
        Self {
            kind,
            video: video.to_path_buf(),
            screenshot: screenshot.to_path_buf(),
            at_second,
        }
    }
}

impl FfmpegErrorKind {
    /// A short, human-readable label for the failure, including the exit code
    /// when `ffmpeg` exited non-zero.
    fn label(&self) -> String {
        match self {
            Self::SpawnFailed => "could not start `ffmpeg`".to_string(),
            Self::Exited(code) => format!("`ffmpeg` exited with status {code}"),
            Self::ForwardOutput => "could not forward `ffmpeg` output".to_string(),
            Self::NoOutput => "`ffmpeg` succeeded but wrote no output file".to_string(),
        }
    }
}

/// Makes a screenshot of the video with the same name ending in `.png`;
/// returns the filename of the screenshot.
///
/// If a screenshot already exists, it is reused as-is.
pub fn bg_from_video(
    ffmpeg_bin: impl AsRef<std::ffi::OsStr>,
    video_fn: impl AsRef<std::path::Path>,
    at_second: usize,
) -> Result<PathBuf, FfmpegError> {
    let video_fn = PathBuf::from(video_fn.as_ref());
    let mut screenshot_fn = video_fn.clone();
    screenshot_fn.set_extension("png");
    if screenshot_fn.exists() {
        println!("screenshot file exists, skipping {:?}", screenshot_fn);
    } else {
        let error = |kind| FfmpegError::new(kind, &video_fn, &screenshot_fn, at_second);
        let output = Command::new(ffmpeg_bin.as_ref())
            .args([
                "-i",
                &video_fn.to_string_lossy(),
                "-ss",
                &at_second.to_string(),
                "-vframes",
                "1",
                "-y",
                &screenshot_fn.to_string_lossy(),
            ])
            .output()
            .or_raise(|| error(FfmpegErrorKind::SpawnFailed))?;
        // Forward ffmpeg's own output so the real cause is visible to the
        // user regardless of how the error is reported further up.
        io::stdout()
            .write_all(&output.stdout)
            .or_raise(|| error(FfmpegErrorKind::ForwardOutput))?;
        io::stderr()
            .write_all(&output.stderr)
            .or_raise(|| error(FfmpegErrorKind::ForwardOutput))?;
        if !output.status.success() {
            bail!(error(FfmpegErrorKind::Exited(
                output.status.code().unwrap_or(-1)
            )));
        }
        // `ffmpeg` can exit with status 0 while encoding nothing ("Output
        // file is empty, nothing was encoded"), so verify the file was
        // actually written.
        ensure!(
            std::fs::metadata(&screenshot_fn).is_ok_and(|m| m.is_file() && m.len() > 0),
            error(FfmpegErrorKind::NoOutput)
        );
    }
    Ok(screenshot_fn)
}
