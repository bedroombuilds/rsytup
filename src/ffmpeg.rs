//! ffmpeg helper functions
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use std::io::{self, Write};
use std::process::Command;

/// makes a screenshot of the video with the same same name ending in png
/// returns filename of screenshot.
pub fn bg_from_video(
    ffmpeg_bin: impl AsRef<std::ffi::OsStr>,
    video_fn: impl AsRef<std::path::Path>,
    at_second: usize,
) -> std::path::PathBuf {
    let video_fn = std::path::PathBuf::from(video_fn.as_ref());
    let mut screenshot_fn = video_fn.clone();
    screenshot_fn.set_extension("png");
    if !screenshot_fn.exists() {
        let output = Command::new(&ffmpeg_bin)
            .args(&[
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
            .expect("failed to execute ffmpeg");
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();
        // TODO: check for "Output file is empty, nothing was encoded" error
        // problem: status is successful in this case!!
        // most stable solution is to verify change-date for screenshot_fn?
        assert!(output.status.success());
    } else {
        println!("screenshot file exists, skipping {:?}", screenshot_fn);
    }
    screenshot_fn
}
