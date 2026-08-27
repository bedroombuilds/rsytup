// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
//! YouTube API connection and helper functions
//!
//! Built on the official `google-youtube3` api client (google-apis-rs).

mod oauth_flow;

use crate::options::{ChangeMode, UploadOptions};
use google_youtube3::api;
use google_youtube3::{hyper_rustls, hyper_util, yup_oauth2};
use std::path::Path;
use std::str::FromStr;

/// Official youtube api v3 hub, talking https via a rustls connector
pub type Hub = google_youtube3::YouTube<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
>;

/// the scopes requested for all youtube api calls
const SCOPES: [api::Scope; 2] = [api::Scope::Upload, api::Scope::ForceSsl];

/// Create a new https client and oauth authenticator and build the api hub
pub async fn new_hub() -> Hub {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .unwrap()
        .https_or_http()
        .enable_http2()
        .build();
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(connector.clone());
    // Put your client secret in the working directory!
    let sec = yup_oauth2::read_application_secret("client_secret.json")
        .await
        .expect("client secret couldn't be read.");
    let auth = yup_oauth2::InstalledFlowAuthenticator::with_client(
        sec,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        yup_oauth2::client::CustomHyperClientBuilder::from(
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector),
        ),
    )
    .persist_tokens_to_disk("tokencache.json")
    // use our custom flow delegate instead of default
    .flow_delegate(Box::new(oauth_flow::InstalledFlowBrowserDelegate))
    .build()
    .await
    .expect("InstalledFlowAuthenticator failed to build");

    google_youtube3::YouTube::new(client, auth)
}

/// parse a publish datetime string into an UTC DateTime
fn parse_publish_datetime(s: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    for fmt in ["%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%d %H:%M:%SZ"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(naive.and_utc());
        }
    }
    anyhow::bail!("invalid publish datetime: {s}");
}

/// List most popular videos of the whole of youtube
pub(crate) async fn video_list(cl: &Hub) {
    let part = vec![
        "id".to_string(),
        "contentDetails".to_string(),
        "snippet".to_string(),
    ];
    let resp = cl
        .videos()
        .list(&part)
        .add_scopes(SCOPES)
        .chart("mostPopular")
        .doit()
        .await
        .expect("listing your yt failed!");
    if let Some(videos) = resp.1.items {
        for f in videos {
            println!(
                "{} => duration: {} title: '{}'",
                f.id.unwrap(),
                f.content_details
                    .map(|cd| cd.duration.unwrap_or_else(|| "n.a.".to_string()))
                    .unwrap(),
                f.snippet
                    .map(|s| s.title.unwrap_or_else(|| "n.a.".to_string()))
                    .unwrap()
            );
        }
    }
}

/// Upload a local file to your YouTube channel.
pub(crate) async fn upload_file(cl: &Hub, options: &UploadOptions) -> anyhow::Result<String> {
    let vsnip = api::VideoSnippet {
        title: Some(options.title()),
        description: Some(options.description.clone()),
        tags: Some(options.tags()),
        category_id: Some((options.category as u8).to_string()),
        default_language: Some("en".to_string()),
        default_audio_language: Some("en".to_string()),
        ..Default::default()
    };
    let vstatus = api::VideoStatus {
        privacy_status: Some(options.privacy_status.to_string()),
        publish_at: Some(parse_publish_datetime(&options.publish_datetime()?)?),
        self_declared_made_for_kids: Some(false),
        ..Default::default()
    };
    let video = api::Video {
        snippet: Some(vsnip),
        status: Some(vstatus),
        ..Default::default()
    };
    let file = std::fs::File::open(&options.file)?;
    let (_, resp) = cl
        .videos()
        .insert(video)
        .add_part("id")
        .add_scopes(SCOPES)
        .upload_resumable(file, mime::Mime::from_str("application/octet-stream")?)
        .await?;
    println!("Video-ID: {:?}, Resp:{:?}", resp.id.as_ref(), resp);
    Ok(String::from(resp.id.as_ref().unwrap()))
}

/// Upload a Thumbnail for a videofile.
pub(crate) async fn upload_thumbnail(
    cl: &Hub,
    video_id: &str,
    thumbnail: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let file = std::fs::File::open(thumbnail.as_ref())?;
    let (_, resp) = cl
        .thumbnails()
        .set(video_id)
        .add_scopes(SCOPES)
        .upload_resumable(file, mime::Mime::from_str("application/octet-stream")?)
        .await?;
    println!("Thumbnail-Resp:{:?}", resp);
    Ok(())
}

/// add Video to playlist
pub(crate) async fn add_to_playlist(
    cl: &Hub,
    options: &UploadOptions,
    video_id: &str,
) -> anyhow::Result<()> {
    let item = api::PlaylistItem {
        snippet: Some(api::PlaylistItemSnippet {
            playlist_id: Some(options.playlist_id.as_ref().unwrap().into()),
            resource_id: Some(api::ResourceId {
                kind: Some("youtube#video".to_string()),
                video_id: Some(video_id.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let (_, resp) = cl
        .playlist_items()
        .insert(item)
        .add_scopes(SCOPES)
        .doit()
        .await?;
    println!("resp {:?}", resp);
    Ok(())
}

/// change videos description text
/// in order to update only the snippet.description we need to fetch the full video snippet data
/// and replace the description before issuing the update command, otherwise e.g. missing
/// snippet.tags info would be reset to default! to update a snippet title and category_id are
/// mandatory
pub(crate) async fn change_description(
    cl: &Hub,
    video_id: &str,
    description: &str,
    change_mode: ChangeMode,
) -> anyhow::Result<()> {
    let part = vec!["snippet".to_string()];
    let resp = cl
        .videos()
        .list(&part)
        .add_id(video_id)
        .add_scopes(SCOPES)
        .doit()
        .await?;
    if let Some(video) = resp.1.items.as_ref().unwrap().iter().take(1).next() {
        let mut vsnip = video.snippet.as_ref().unwrap().clone();
        let old_desc = vsnip.description.unwrap().clone();
        let new_desc = match change_mode {
            ChangeMode::Append => format!("{}{}", old_desc.trim_end(), description),
            ChangeMode::Replace => description.to_string(),
            ChangeMode::Prepend => format!("{}{}", description, old_desc.trim_end()),
        };
        vsnip.description = Some(new_desc);
        let video = api::Video {
            id: Some(video_id.to_string()),
            snippet: Some(vsnip),
            ..Default::default()
        };
        let (_, resp) = cl.videos().update(video).add_scopes(SCOPES).doit().await?;
        println!("resp {:?}", resp);
    }
    Ok(())
}

pub async fn uploaded_video_list(cl: &Hub) -> anyhow::Result<Vec<YtVid>> {
    let part = vec!["contentDetails".to_string()];
    let resp = cl
        .channels()
        .list(&part)
        .mine(true)
        .add_scopes(SCOPES)
        .doit()
        .await
        .expect("listing your yt failed!");
    // we get the id of the first channels uploads playlist, pseudocode:
    // resp.items[0].content_details.related_playlists.uploads
    if let Some(channel) = resp
        .1
        .items
        .and_then(|channels| channels.into_iter().next())
    {
        let channel_id = channel
            .content_details
            .unwrap()
            .related_playlists
            .unwrap()
            .uploads
            .unwrap();
        println!("{:#?}", channel_id);
        let cl = new_hub().await;
        return list_playlist(&cl, &channel_id).await;
    }
    Ok(vec![])
}

/// Youtube Video minimal information
pub struct YtVid {
    pub id: String,
    pub title: String,
    pub description: String,
}

impl YtVid {
    pub async fn from_id(cl: &Hub, video_id: &str) -> anyhow::Result<YtVid> {
        let part = vec!["snippet".to_string()];
        let resp = cl
            .videos()
            .list(&part)
            .add_id(video_id)
            .add_scopes(SCOPES)
            .doit()
            .await?;
        if let Some(video) = resp.1.items.as_ref().unwrap().iter().take(1).next() {
            let vsnip = video.snippet.as_ref().unwrap().clone();
            Ok(Self {
                id: video_id.to_string(),
                title: vsnip.title.unwrap(),
                description: vsnip.description.unwrap(),
            })
        } else {
            Ok(Self {
                id: video_id.to_string(),
                title: "".to_string(),
                description: "".to_string(),
            })
        }
    }
}

/// list all Video in playlist
/// this will loop and fetch 10 items from the list until complete
/// returns a list of youtube videos
pub(crate) async fn list_playlist(cl: &Hub, playlist_id: &str) -> anyhow::Result<Vec<YtVid>> {
    let part = vec!["snippet".to_string()];
    let mut page_token: Option<String> = None;
    let mut all_videos = vec![];
    loop {
        let mut call = cl
            .playlist_items()
            .list(&part)
            .playlist_id(playlist_id)
            .max_results(10) // max is 50
            .add_scopes(SCOPES);
        if let Some(token) = &page_token {
            call = call.page_token(token);
        }
        let resp = call.doit().await?;
        if let Some(videos) = resp.1.items {
            for f in videos {
                let (video_id, title, description) = f
                    .snippet
                    .map(|s| {
                        let t = s.title.unwrap_or_else(|| "n.a.".to_string());
                        let d = s.description.unwrap_or_else(|| "n.a.".to_string());
                        (s.resource_id.unwrap().video_id.unwrap(), t, d)
                    })
                    .unwrap();
                println!("{} => title: '{}'", video_id, title);
                all_videos.push(YtVid {
                    id: video_id.clone(),
                    title: title.clone(),
                    description: description.clone(),
                });
            }
        }
        match resp.1.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    Ok(all_videos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_publish_datetime() {
        let d = parse_publish_datetime("2026-09-04T08:00:00Z").unwrap();
        assert_eq!(d.to_rfc3339(), "2026-09-04T08:00:00+00:00");
        // space separated variant as produced by NaiveDateTime Debug formatting
        let d = parse_publish_datetime("2026-09-04 08:00:00Z").unwrap();
        assert_eq!(d.to_rfc3339(), "2026-09-04T08:00:00+00:00");
        assert!(parse_publish_datetime("nonsense").is_err());
    }
}
