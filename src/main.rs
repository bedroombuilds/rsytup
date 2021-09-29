//! rsytup - Rust YouTube uploader
//! a tool to automate common actions when uploading a video to youtube
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021 Michael Kefeder
use std::path::PathBuf;
use structopt::StructOpt;

mod date_compute;
mod ffmpeg;
mod options;
mod thumbnail;
mod youtube;

use options::{Command, Options};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let options = Options::from_args();
    match options.cmd {
        Command::Upload(mut options) => {
            if options.pretend {
                println!("publish-at: {:?}", options.publish_at());
                println!("publish-datetime: {}", options.publish_datetime()?);
                if let Ok(episode_nr) = options.episode_nr() {
                    println!("episode_nr: {} (0x{:X})", episode_nr, episode_nr);
                    println!(r#"youtube-title: "{:X}. {}""#, episode_nr, options.title());
                } else {
                    println!("episode_nr: n.a.");
                    println!(r#"youtube-title: "{}""#, options.title());
                }
                println!("catgegory: {:?}", options.category);
                println!("thumb-title: {:?}", options.title());
                println!("youtube-description: {}", &options.description);
                println!("youtube-tags: {:?}", &options.tags());
                std::process::exit(0);
            }
            // if no thumbnail given, check if video-filename with .jpg extension exists (=default
            // thumbnail), if not make one with that filename
            if options.thumbnail.is_none() {
                let mut thumb_path = PathBuf::from(&options.file);
                thumb_path.set_extension("jpg");
                if !thumb_path.exists() {
                    let screenshot_fn = ffmpeg::bg_from_video(
                        &options.ffmpeg_bin,
                        &options.file,
                        options.thumb_second,
                    );
                    thumbnail::make_thumbnail(
                        &thumb_path,
                        &screenshot_fn,
                        &options.thumbnail_watermark,
                        &options.title(),
                    );
                }
                options.thumbnail = Some(thumb_path);
            }
            println!("thumbnail-path: {:?}", &options.thumbnail);
        }
        Command::List(options) => {
            if options.publish_methods {
                crate::options::print_publish_date_enum();
                std::process::exit(0);
            }
            if options.yt_top5 {
                let mut cl = youtube::video_service().await;
                youtube::video_list(&mut cl).await;
                std::process::exit(1);
            }
            if options.uploaded {
                eprintln!("Not yet implemented");
                std::process::exit(1);
            }
        }
        Command::Update(_options) => {
            eprintln!("Not yet implemented");
        }
    }
    Ok(())
}
