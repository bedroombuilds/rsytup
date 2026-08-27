//! `rsytup` - Rust YouTube uploader
//! a tool to automate common actions when uploading a video to YouTube
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use clap::Parser;
use std::path::PathBuf;

mod date_compute;
mod ffmpeg;
mod options;
mod thumbnail;
mod youtube;

use options::{Command, Options};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let options = Options::parse();
    match options.cmd {
        Command::Upload(mut options) => {
            if options.pretend {
                println!("publish-at: {:?}", options.publish_at);
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
                println!("youtube-description: {}", options.description);
                println!("youtube-tags: {:?}", options.tags());
                std::process::exit(0);
            }
            // If no thumbnail given, check if video-filename with .jpg extension exists (=default
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
            println!("thumbnail-path: {:?}", options.thumbnail);
            let cl = youtube::new_hub().await;
            let video_id = youtube::upload_file(&cl, &options).await?;
            println!("upload video_id {:?}", video_id);

            if let Some(ref thumbnail) = options.thumbnail {
                let _ = youtube::upload_thumbnail(&cl, &video_id, thumbnail).await;
            }
            if options.playlist_id.is_some() {
                let _ = youtube::add_to_playlist(&cl, &options, &video_id).await;
            }
        }
        Command::List(options) => {
            if options.publish_methods {
                crate::options::print_publish_date_enum();
                std::process::exit(0);
            }
            if options.yt_top5 {
                let cl = youtube::new_hub().await;
                if options.json {
                    todo!("adapt according to uploads");
                } else {
                    youtube::video_list_top5(&cl).await;
                }
                std::process::exit(1);
            }
            if options.uploaded {
                let hub = youtube::new_hub().await;
                let uploads = youtube::list::list_uploads(&hub, None).await?;
                if options.json {
                    youtube::list::print_json(&uploads)?;
                } else {
                    youtube::list::print_table(&uploads);
                }
                std::process::exit(1);
            }
            if let Some(ref video_id) = options.video_id {
                let cl = youtube::new_hub().await;
                let video = vec![youtube::list::YtVideoInfo::from_id(&cl, video_id).await?];
                if options.json {
                    youtube::list::print_json(&video)?;
                } else {
                    youtube::list::print_table(&video);
                }
                std::process::exit(1);
            }
        }
        Command::Update(options) => {
            let cl = youtube::new_hub().await;
            let vids = if options.video_id == "uploaded" {
                youtube::list::list_uploads(&cl, None).await?
            } else {
                vec![youtube::list::YtVideoInfo::from_id(&cl, &options.video_id).await?]
            };
            if let Some(new_thumb) = options.generate_thumbnail {
                let entries = std::fs::read_dir(&new_thumb)?
                    .map(|res| res.map(|e| e.path()))
                    .collect::<Result<Vec<_>, std::io::Error>>()?;
                println!("{:#?}", entries);
                let mov_ext = Some(std::ffi::OsStr::new("mov"));
                let thumbnail_cl = youtube::new_hub().await;
                for v in vids {
                    if let Some((episode_nr, ep_title)) = &v.title.split_once('.') {
                        // Text on thumbnail is without episode number and series info
                        let ep_title = match ep_title.split_once('-') {
                            Some((t, _)) => t.trim(),
                            None => ep_title,
                        };
                        let video_fn: PathBuf = entries
                            .iter()
                            .filter(|vfn| {
                                vfn.file_name()
                                    .map(|x| x.to_str().unwrap().starts_with(episode_nr))
                                    .unwrap()
                                    && vfn.extension() == mov_ext
                            })
                            .take(1)
                            .collect();
                        println!("Video {} {:?}", episode_nr, video_fn);
                        let mut thumb_path = PathBuf::from(&video_fn);
                        thumb_path.set_extension("jpg");
                        let screenshot_fn = ffmpeg::bg_from_video(
                            &options.ffmpeg_bin,
                            &video_fn,
                            options.thumb_second,
                        );
                        thumbnail::make_thumbnail(
                            &thumb_path,
                            &screenshot_fn,
                            &options.thumbnail_watermark,
                            ep_title,
                        );
                        let _ = youtube::upload_thumbnail(&thumbnail_cl, &v.id, thumb_path).await;
                    }
                }
            } else if let Some(desc) = options.description {
                let new_desc = std::fs::read_to_string(&desc)?;
                for v in vids {
                    youtube::change_description(&cl, &v.id, &new_desc, options.change_desc).await?;
                }
            } else if let Some(playlist_id) = options.add_to_playlist {
                eprintln!(
                    "at some point in time this will add a video_id to a playlist: {}",
                    playlist_id
                );
            } else {
                eprintln!("not implemented");
            }
        }
    }
    Ok(())
}
