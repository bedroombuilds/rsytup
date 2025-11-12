// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
//! YouTube API connection and helper functions

mod oauth_flow;
mod youtube_v3_types;
use youtube_v3_types as yt;

use crate::options::{ChangeMode, UploadOptions};
use async_google_apis_common as common;
use std::rc::Rc;

/// Create a new HTTPS client.
fn https_client() -> common::TlsClient {
    let conn = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http2()
        .build();
    hyper::Client::builder().build(conn)
}

async fn service_basics() -> (
    hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
    common::yup_oauth2::authenticator::Authenticator<
        hyper_rustls::HttpsConnector<hyper::client::HttpConnector>,
    >,
) {
    let https = https_client();
    // Put your client secret in the working directory!
    let sec = common::yup_oauth2::read_application_secret("client_secret.json")
        .await
        .expect("client secret couldn't be read.");
    let auth = common::yup_oauth2::InstalledFlowAuthenticator::builder(
        sec,
        common::yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
    )
    .persist_tokens_to_disk("tokencache.json")
    // use our custom flow delegate instead of default
    .flow_delegate(Box::new(oauth_flow::InstalledFlowBrowserDelegate))
    .hyper_client(https.clone())
    .build()
    .await
    .expect("InstalledFlowAuthenticator failed to build");

    (https, auth)
}

pub(crate) async fn video_service() -> yt::VideosService {
    let (https, auth) = service_basics().await;
    let scopes = vec![
        yt::YoutubeScopes::YoutubeUpload,
        yt::YoutubeScopes::YoutubeForceSsl,
    ];
    let mut cl = yt::VideosService::new(https, Rc::new(auth));
    cl.set_scopes(&scopes);
    cl
}

pub async fn thumbnail_service() -> yt::ThumbnailsService {
    let (https, auth) = service_basics().await;
    let scopes = vec![
        yt::YoutubeScopes::YoutubeUpload,
        yt::YoutubeScopes::YoutubeForceSsl,
    ];
    let mut cl = yt::ThumbnailsService::new(https, Rc::new(auth));
    cl.set_scopes(&scopes);
    cl
}

pub async fn playlist_service() -> yt::PlaylistItemsService {
    let (https, auth) = service_basics().await;
    let scopes = vec![
        yt::YoutubeScopes::YoutubeUpload,
        yt::YoutubeScopes::YoutubeForceSsl,
    ];
    let mut cl = yt::PlaylistItemsService::new(https, Rc::new(auth));
    cl.set_scopes(&scopes);
    cl
}

pub async fn channels_service() -> yt::ChannelsService {
    let (https, auth) = service_basics().await;
    let scopes = vec![
        yt::YoutubeScopes::YoutubeUpload,
        yt::YoutubeScopes::YoutubeForceSsl,
    ];
    let mut cl = yt::ChannelsService::new(https, Rc::new(auth));
    cl.set_scopes(&scopes);
    cl
}

pub(crate) async fn video_list(cl: &mut yt::VideosService) {
    // By default, list most popular videos
    let general_params = yt::YoutubeParams {
        fields: Some("*".to_string()),
        ..Default::default()
    };
    let p = yt::VideosListParams {
        youtube_params: Some(general_params),
        part: "id,contentDetails,snippet".into(),
        chart: Some("mostPopular".to_string()),
        ..Default::default()
    };

    let resp = cl.list(&p).await.expect("listing your yt failed!");
    if let Some(videos) = resp.items {
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
pub(crate) async fn upload_file(
    cl: &mut yt::VideosService,
    options: &UploadOptions,
) -> anyhow::Result<String> {
    let general_params = yt::YoutubeParams {
        fields: Some("*".to_string()),
        ..Default::default()
    };
    let vsnip = yt::VideoSnippet {
        title: Some(options.title()),
        description: Some(options.description.clone()),
        tags: Some(options.tags()),
        category_id: Some((options.category as u8).to_string()),
        default_language: Some("en".to_string()),
        default_audio_language: Some("en".to_string()),
        ..Default::default()
    };
    let vstatus = yt::VideoStatus {
        privacy_status: Some(options.privacy_status.to_string()),
        publish_at: Some(options.publish_datetime().unwrap()),
        self_declared_made_for_kids: Some(false),
        ..Default::default()
    };
    let video = yt::Video {
        snippet: Some(vsnip),
        status: Some(vstatus),
        ..Default::default()
    };
    let params = yt::VideosInsertParams {
        youtube_params: Some(general_params.clone()),
        part: "id,status,snippet".into(),
        ..Default::default()
    };
    let resumable = cl.insert_resumable_upload(&params, &video).await?;
    let tf = tokio::fs::OpenOptions::new()
        .read(true)
        .open(options.file.clone())
        .await?;
    let resp = resumable.upload_file(tf).await?;
    println!("Video-ID: {:?}, Resp:{:?}", resp.id.as_ref(), resp);
    Ok(String::from(resp.id.as_ref().unwrap()))
}

/// Upload a Thumbnail for a videofile.
pub(crate) async fn upload_thumbnail(
    cl: &mut yt::ThumbnailsService,
    video_id: &str,
    thumbnail: impl AsRef<std::path::Path>,
) -> anyhow::Result<()> {
    let params = yt::ThumbnailsSetParams {
        video_id: video_id.into(),
        ..Default::default()
    };
    let resumable = cl.set_resumable_upload(&params).await?;
    let tf = tokio::fs::OpenOptions::new()
        .read(true)
        .open(thumbnail.as_ref())
        .await?;
    let resp = resumable.upload_file(tf).await?;
    println!("Thumbnail-Resp:{:?}", resp);
    Ok(())
}

/// add Video to playlist
pub(crate) async fn add_to_playlist(
    cl: &mut yt::PlaylistItemsService,
    options: &UploadOptions,
    video_id: &str,
) -> anyhow::Result<()> {
    let params = yt::PlaylistItemsInsertParams {
        part: "snippet".into(),
        ..Default::default()
    };
    let item = yt::PlaylistItem {
        snippet: Some(yt::PlaylistItemSnippet {
            playlist_id: Some(options.playlist_id.as_ref().unwrap().into()),
            resource_id: Some(yt::ResourceId {
                kind: Some("youtube#video".to_string()),
                video_id: Some(video_id.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let resp = cl.insert(&params, &item).await?;
    println!("resp {:?}", resp);
    Ok(())
}

/// change videos description text
/// in order to update only the snippet.description we need to fetch the full video snippet data
/// and replace the description before issuing the update command, otherwise e.g. missing
/// snippet.tags info would be reset to default! to update a snippet title and category_id are
/// mandatory
pub(crate) async fn change_description(
    cl: &mut yt::VideosService,
    video_id: &str,
    description: &str,
    change_mode: ChangeMode,
) -> anyhow::Result<()> {
    let params = yt::VideosListParams {
        id: Some(video_id.to_string()),
        part: "snippet".into(),
        ..Default::default()
    };
    let resp = cl.list(&params).await?;
    if let Some(video) = resp.items.as_ref().unwrap().iter().take(1).next() {
        let mut vsnip = video.snippet.as_ref().unwrap().clone();
        let old_desc = vsnip.description.unwrap().clone();
        let new_desc = match change_mode {
            ChangeMode::Append => format!("{}{}", old_desc.trim_end(), description),
            ChangeMode::Replace => description.to_string(),
            ChangeMode::Prepend => format!("{}{}", description, old_desc.trim_end()),
        };
        vsnip.description = Some(new_desc);
        let params = yt::VideosUpdateParams {
            part: "id,snippet".into(),
            ..Default::default()
        };
        let video = yt::Video {
            id: Some(video_id.to_string()),
            snippet: Some(vsnip),
            ..Default::default()
        };
        let resp = cl.update(&params, &video).await?;
        println!("resp {:?}", resp);
    }
    Ok(())
}

pub async fn uploaded_video_list(cl: &mut yt::ChannelsService) -> anyhow::Result<Vec<YtVid>> {
    let p = yt::ChannelsListParams {
        mine: Some(true),
        part: "contentDetails".into(),
        ..Default::default()
    };
    let resp = cl.list(&p).await.expect("listing your yt failed!");
    // we get the id fo the first channels playlist, pseudocode:
    // resp.items[0].content_details.related_playlists.uploads
    if let Some(channels) = resp.items {
        if let Some(channel) = channels.into_iter().take(1).next() {
            let channel_id = channel
                .content_details
                .unwrap()
                .related_playlists
                .unwrap()
                .uploads
                .unwrap();
            println!("{:#?}", channel_id);
            let mut cl = playlist_service().await;
            return list_playlist(&mut cl, &channel_id).await;
        }
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
    pub async fn from_id(cl: &mut yt::VideosService, video_id: &str) -> anyhow::Result<YtVid> {
        let params = yt::VideosListParams {
            id: Some(video_id.to_string()),
            part: "snippet".into(),
            ..Default::default()
        };
        let resp = cl.list(&params).await?;
        if let Some(video) = resp.items.as_ref().unwrap().iter().take(1).next() {
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
pub(crate) async fn list_playlist(
    cl: &mut yt::PlaylistItemsService,
    playlist_id: &str,
) -> anyhow::Result<Vec<YtVid>> {
    let mut params = yt::PlaylistItemsListParams {
        part: "snippet".into(),
        playlist_id: Some(playlist_id.to_string()),
        max_results: Some(10), // max is 50
        ..Default::default()
    };
    let mut all_videos = vec![];
    loop {
        let resp = cl.list(&params).await?;
        if let Some(videos) = resp.items {
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
        match resp.next_page_token {
            Some(token) => params.page_token = Some(token),
            None => break,
        }
    }
    Ok(all_videos)
}
