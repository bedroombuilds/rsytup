//! `rsytup` - Rust YouTube uploader
//! a tool to automate common actions when uploading a video to YouTube
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

mod date_compute;
mod ffmpeg;
mod options;
mod thumbnail;
mod youtube;

use exn::{Exn, Frame, ResultExt};
use options::{Command, Options};
use thiserror::Error;
use youtube::YoutubeError;

/// Top-level error: names the command-level operation that failed.
///
/// The module errors below it carry the structured details; `main` prints the
/// whole frame tree via `Debug`, plus a hint when the user can act on it.
#[derive(Debug, Error)]
#[error("{0}")]
struct AppError(String);

/// Find the outermost error of type `T` anywhere in the frame tree.
fn find_error<T: std::error::Error + 'static>(frame: &Frame) -> Option<&T> {
    frame
        .error()
        .downcast_ref::<T>()
        .or_else(|| frame.children().iter().find_map(find_error))
}

/// Print a YouTube failure that does not abort the run, with its hint.
fn warn(err: &Exn<YoutubeError>) {
    eprintln!("warning: {err:?}");
    if let Some(hint) = err.kind().hint() {
        eprintln!("hint: {hint}");
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err:?}");
            if let Some(hint) =
                find_error::<YoutubeError>(err.frame()).and_then(|e| e.kind().hint())
            {
                eprintln!("hint: {hint}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Exn<AppError>> {
    env_logger::init();
    let options = Options::parse();
    match options.cmd {
        Command::Upload(mut options) => {
            let upload_error = || AppError(format!("uploading {}", options.file.display()));
            if options.pretend {
                println!("publish-at: {:?}", options.publish_at);
                println!(
                    "publish-datetime: {}",
                    options.publish_datetime().or_raise(upload_error)?
                );
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
                return Ok(());
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
                    )
                    .or_raise(upload_error)?;
                    thumbnail::make_thumbnail(
                        &thumb_path,
                        &screenshot_fn,
                        &options.thumbnail_watermark,
                        &options.title(),
                    )
                    .or_raise(upload_error)?;
                }
                options.thumbnail = Some(thumb_path);
            }
            println!("thumbnail-path: {:?}", options.thumbnail);
            let cl = youtube::new_hub().await.or_raise(upload_error)?;
            let video_id = youtube::upload_file(&cl, &options)
                .await
                .or_raise(upload_error)?;
            println!("upload video_id {:?}", video_id);

            // The video is already uploaded at this point, so a failing
            // thumbnail or playlist step is reported but does not abort.
            if let Some(ref thumbnail) = options.thumbnail
                && let Err(err) = youtube::upload_thumbnail(&cl, &video_id, thumbnail).await
            {
                warn(&err);
            }
            if options.playlist_id.is_some()
                && let Err(err) = youtube::add_to_playlist(&cl, &options, &video_id).await
            {
                warn(&err);
            }
        }
        Command::List(options) => {
            if options.publish_methods {
                crate::options::print_publish_date_enum();
                return Ok(());
            }
            if options.yt_top5 {
                let top5_error = || AppError("listing the most popular videos".to_string());
                let cl = youtube::new_hub().await.or_raise(top5_error)?;
                if options.json {
                    todo!("adapt according to uploads");
                } else {
                    youtube::video_list_top5(&cl).await.or_raise(top5_error)?;
                }
                return Ok(());
            }
            if options.uploaded {
                let uploaded_error = || AppError("listing uploaded videos".to_string());
                let hub = youtube::new_hub().await.or_raise(uploaded_error)?;
                let uploads = youtube::list::list_uploads(&hub, None)
                    .await
                    .or_raise(uploaded_error)?;
                if options.json {
                    youtube::list::print_json(&uploads);
                } else {
                    youtube::list::print_table(&uploads);
                }
                return Ok(());
            }
            if let Some(ref video_id) = options.video_id {
                let video_error = || AppError(format!("listing video {video_id}"));
                let cl = youtube::new_hub().await.or_raise(video_error)?;
                let video = vec![
                    youtube::list::YtVideoInfo::from_id(&cl, video_id)
                        .await
                        .or_raise(video_error)?,
                ];
                if options.json {
                    youtube::list::print_json(&video);
                } else {
                    youtube::list::print_table(&video);
                }
                return Ok(());
            }
        }
        Command::Update(options) => {
            let update_error = || AppError(format!("updating video {}", options.video_id));
            let cl = youtube::new_hub().await.or_raise(update_error)?;
            let vids = if options.video_id == "uploaded" {
                youtube::list::list_uploads(&cl, None)
                    .await
                    .or_raise(update_error)?
            } else {
                vec![
                    youtube::list::YtVideoInfo::from_id(&cl, &options.video_id)
                        .await
                        .or_raise(update_error)?,
                ]
            };
            if let Some(new_thumb) = options.generate_thumbnail {
                let read_dir_error = || {
                    AppError(format!(
                        "reading video directory {} for thumbnails",
                        new_thumb.display()
                    ))
                };
                let entries = std::fs::read_dir(&new_thumb)
                    .or_raise(read_dir_error)?
                    .map(|res| res.map(|e| e.path()))
                    .collect::<Result<Vec<_>, std::io::Error>>()
                    .or_raise(read_dir_error)?;
                println!("{:#?}", entries);
                let mov_ext = Some(std::ffi::OsStr::new("mov"));
                let thumbnail_cl = youtube::new_hub().await.or_raise(update_error)?;
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
                        let thumbnail_error =
                            || AppError(format!("generating thumbnail for video {}", v.id));
                        let screenshot_fn = ffmpeg::bg_from_video(
                            &options.ffmpeg_bin,
                            &video_fn,
                            options.thumb_second,
                        )
                        .or_raise(thumbnail_error)?;
                        thumbnail::make_thumbnail(
                            &thumb_path,
                            &screenshot_fn,
                            &options.thumbnail_watermark,
                            ep_title,
                        )
                        .or_raise(thumbnail_error)?;
                        if let Err(err) =
                            youtube::upload_thumbnail(&thumbnail_cl, &v.id, thumb_path).await
                        {
                            warn(&err);
                        }
                    }
                }
            } else if let Some(desc) = options.description {
                let new_desc = std::fs::read_to_string(&desc).or_raise(|| {
                    AppError(format!("reading description file {}", desc.display()))
                })?;
                for v in vids {
                    youtube::change_description(&cl, &v.id, &new_desc, options.change_desc)
                        .await
                        .or_raise(|| AppError(format!("changing description of video {}", v.id)))?;
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
