// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
//! YouTube API connection and helper functions
//!
//! Built on the official `google-youtube3` API client ([`google-apis-rs`](https://github.com/google-apis-rs/)).

pub mod list;
mod oauth_flow;

use crate::options::{ChangeMode, UploadOptions};
use exn::{Exn, OptionExt, Result, ResultExt, bail};
use thiserror::Error;

use google_youtube3::api;
use google_youtube3::{hyper_rustls, hyper_util, yup_oauth2};
use std::path::Path;

/// Official YouTube API v3 hub, talking HTTPS via a `rustls` connector
pub type Hub = google_youtube3::YouTube<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
>;

/// The scopes requested for all YouTube API calls
const SCOPES: [api::Scope; 2] = [api::Scope::Upload, api::Scope::ForceSsl];

/// What went wrong, categorized by what a caller *can do* about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeErrorKind {
    /// The API client could not be set up: TLS roots, `client_secret.json` or
    /// the OAuth flow. Fix the local setup.
    Setup,
    /// The stored OAuth token is missing, expired or revoked. Sign in again.
    Unauthorized,
    /// The daily API quota is used up. Wait for the quota reset.
    QuotaExceeded,
    /// A network failure or a server-side error (429, 5xx). Retrying may help.
    Temporary,
    /// The API rejected the request for any other reason. Retrying won't help.
    Api,
    /// A local file (video or thumbnail) could not be read.
    File,
    /// The upload options are incomplete or invalid. Fix the command line.
    InvalidOptions,
    /// The requested video does not exist or is not visible to the account.
    NotFound,
    /// The authenticated account has no channel, or the channel exposes no
    /// uploads playlist. Use a different account.
    NoUploads,
    /// The API answered, but without data it is documented to always include.
    UnexpectedResponse,
}

impl YoutubeErrorKind {
    /// A short, human-readable label for the failure.
    fn label(&self) -> &'static str {
        match self {
            Self::Setup => "could not set up the YouTube API client",
            Self::Unauthorized => "not authorized",
            Self::QuotaExceeded => "API quota exceeded",
            Self::Temporary => "temporary failure",
            Self::Api => "YouTube API request failed",
            Self::File => "could not read local file",
            Self::InvalidOptions => "invalid options",
            Self::NotFound => "not found",
            Self::NoUploads => "no uploads playlist",
            Self::UnexpectedResponse => "unexpected API response",
        }
    }

    /// What the user can do about the failure, if there is anything.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Setup => Some("put a valid client_secret.json into the working directory"),
            Self::Unauthorized => Some("delete tokencache.json to sign in again"),
            Self::QuotaExceeded => {
                Some("the daily YouTube API quota resets at midnight Pacific Time")
            }
            Self::Temporary => Some("this is likely temporary, try again later"),
            Self::NotFound => Some("check the video id and the signed-in account"),
            Self::NoUploads => {
                Some("sign in with an account that has a YouTube channel (delete tokencache.json)")
            }
            Self::Api | Self::File | Self::InvalidOptions | Self::UnexpectedResponse => None,
        }
    }

    /// Classify a failed API call by what the caller can do about it.
    fn from_api(err: &google_youtube3::Error) -> Self {
        use google_youtube3::Error;
        match err {
            Error::HttpError(_) | Error::Io(_) => Self::Temporary,
            Error::MissingToken(_) => Self::Unauthorized,
            // The API's JSON error body: {"error": {"code": 403, "errors": [{"reason": ..}]}}
            Error::BadRequest(body) => {
                let quota = body["error"]["errors"].as_array().is_some_and(|errors| {
                    errors.iter().any(|e| {
                        matches!(
                            e["reason"].as_str(),
                            Some("quotaExceeded" | "dailyLimitExceeded")
                        )
                    })
                });
                if quota {
                    Self::QuotaExceeded
                } else {
                    Self::from_status(body["error"]["code"].as_u64().unwrap_or(0))
                }
            }
            Error::Failure(response) => Self::from_status(response.status().as_u16().into()),
            _ => Self::Api,
        }
    }

    fn from_status(status: u64) -> Self {
        match status {
            401 => Self::Unauthorized,
            404 => Self::NotFound,
            429 | 500..=599 => Self::Temporary,
            _ => Self::Api,
        }
    }
}

/// A failure while talking to the YouTube API.
///
/// Modeled as a flat, kind-based error: [`YoutubeErrorKind`] says what the
/// caller can do about the failure, while `message` names the operation and
/// the ids or paths it was working on.
#[derive(Debug, Error)]
#[error("{message}: {}", self.kind.label())]
pub struct YoutubeError {
    kind: YoutubeErrorKind,
    /// What was being done when the failure happened, including ids and paths.
    message: String,
}

impl YoutubeError {
    fn new(kind: YoutubeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn setup(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::Setup, message)
    }

    fn file(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::File, message)
    }

    fn invalid_options(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::InvalidOptions, message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::NotFound, message)
    }

    fn no_uploads(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::NoUploads, message)
    }

    fn unexpected(message: impl Into<String>) -> Self {
        Self::new(YoutubeErrorKind::UnexpectedResponse, message)
    }

    pub fn kind(&self) -> YoutubeErrorKind {
        self.kind
    }
}

/// Raise a failed API call as a [`YoutubeError`] whose kind is derived from
/// the API's answer, keeping the original error as a child frame.
trait ApiResultExt<T> {
    fn or_raise_api<M: Into<String>>(self, op: impl FnOnce() -> M) -> Result<T, YoutubeError>;
}

impl<T> ApiResultExt<T> for std::result::Result<T, google_youtube3::Error> {
    // A `match` rather than `map_err`: `#[track_caller]` does not reach into
    // closures, so the frames would record this file instead of the call site.
    #[track_caller]
    fn or_raise_api<M: Into<String>>(self, op: impl FnOnce() -> M) -> Result<T, YoutubeError> {
        match self {
            Ok(value) => Ok(value),
            Err(err) => {
                let kind = YoutubeErrorKind::from_api(&err);
                Err(Exn::new(err).raise(YoutubeError::new(kind, op())))
            }
        }
    }
}

/// Create a new HTTPS client and OAuth authenticator and build the API hub
pub async fn new_hub() -> Result<Hub, YoutubeError> {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .or_raise(|| YoutubeError::setup("loading the native TLS root certificates"))?
        .https_or_http()
        .enable_http2()
        .build();
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(connector.clone());
    // Put your client secret in the working directory!
    let sec = yup_oauth2::read_application_secret("client_secret.json")
        .await
        .or_raise(|| {
            YoutubeError::setup("reading client_secret.json from the working directory")
        })?;
    let auth = yup_oauth2::InstalledFlowAuthenticator::with_client(
        sec,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        yup_oauth2::client::CustomHyperClientBuilder::from(
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector),
        ),
    )
    .persist_tokens_to_disk("tokencache.json")
    // Use our custom flow delegate instead of default
    .flow_delegate(Box::new(oauth_flow::InstalledFlowBrowserDelegate))
    .build()
    .await
    .or_raise(|| {
        YoutubeError::setup("building the OAuth authenticator (token cache tokencache.json)")
    })?;

    Ok(google_youtube3::YouTube::new(client, auth))
}

/// Parse a publish datetime string into a UTC Date-Time
fn parse_publish_datetime(s: &str) -> Result<chrono::DateTime<chrono::Utc>, YoutubeError> {
    for fmt in ["%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%d %H:%M:%SZ"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(naive.and_utc());
        }
    }
    bail!(YoutubeError::invalid_options(format!(
        "parsing publish datetime `{s}`"
    )))
}

/// List most popular videos of the whole of YouTube
pub(crate) async fn video_list_top5(cl: &Hub) -> Result<(), YoutubeError> {
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
        .or_raise_api(|| "listing the most popular videos")?;
    if let Some(videos) = resp.1.items {
        for f in videos {
            println!(
                "{} => duration: {} title: '{}'",
                f.id.as_deref().unwrap_or("n.a."),
                f.content_details
                    .and_then(|cd| cd.duration)
                    .unwrap_or_else(|| "n.a.".to_string()),
                f.snippet
                    .and_then(|s| s.title)
                    .unwrap_or_else(|| "n.a.".to_string())
            );
        }
    }
    Ok(())
}

/// Upload a local file to your YouTube channel.
pub(crate) async fn upload_file(cl: &Hub, options: &UploadOptions) -> Result<String, YoutubeError> {
    let video_fn = options.file.display();
    let vsnip = api::VideoSnippet {
        title: Some(options.title()),
        description: Some(options.description.clone()),
        tags: Some(options.tags()),
        category_id: Some((options.category as u8).to_string()),
        default_language: Some("en".to_string()),
        default_audio_language: Some("en".to_string()),
        ..Default::default()
    };
    let publish_datetime = options.publish_datetime().or_raise(|| {
        YoutubeError::invalid_options(format!("computing the publish date of {video_fn}"))
    })?;
    let vstatus = api::VideoStatus {
        privacy_status: Some(options.privacy_status.to_string()),
        publish_at: Some(parse_publish_datetime(&publish_datetime)?),
        self_declared_made_for_kids: Some(false),
        ..Default::default()
    };
    let video = api::Video {
        snippet: Some(vsnip),
        status: Some(vstatus),
        ..Default::default()
    };
    let file = std::fs::File::open(&options.file)
        .or_raise(|| YoutubeError::file(format!("opening video {video_fn}")))?;
    let (_, resp) = cl
        .videos()
        .insert(video)
        .add_part("id")
        .add_scopes(SCOPES)
        .upload_resumable(file, mime::APPLICATION_OCTET_STREAM)
        .await
        .or_raise_api(|| format!("uploading video {video_fn}"))?;
    println!("Video-ID: {:?}, Resp:{:?}", resp.id.as_ref(), resp);
    resp.id.ok_or_raise(|| {
        YoutubeError::unexpected(format!("upload of {video_fn} returned no video id"))
    })
}

/// Upload a Thumbnail for a video-file.
pub(crate) async fn upload_thumbnail(
    cl: &Hub,
    video_id: &str,
    thumbnail: impl AsRef<Path>,
) -> Result<(), YoutubeError> {
    let file = std::fs::File::open(&thumbnail).or_raise(|| {
        YoutubeError::file(format!(
            "opening thumbnail {}",
            thumbnail.as_ref().display()
        ))
    })?;
    let thumbnail = thumbnail.as_ref().display();
    let (_, resp) = cl
        .thumbnails()
        .set(video_id)
        .add_scopes(SCOPES)
        .upload_resumable(file, mime::APPLICATION_OCTET_STREAM)
        .await
        .or_raise_api(|| format!("uploading thumbnail {thumbnail} for video {video_id}"))?;
    println!("Thumbnail-Resp:{:?}", resp);
    Ok(())
}

/// add Video to playlist
pub(crate) async fn add_to_playlist(
    cl: &Hub,
    options: &UploadOptions,
    video_id: &str,
) -> Result<(), YoutubeError> {
    let playlist_id = options.playlist_id.as_ref().ok_or_raise(|| {
        YoutubeError::invalid_options(format!(
            "adding video {video_id} to a playlist without a playlist id"
        ))
    })?;
    let item = api::PlaylistItem {
        snippet: Some(api::PlaylistItemSnippet {
            playlist_id: Some(playlist_id.into()),
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
        .await
        .or_raise_api(|| format!("adding video {video_id} to playlist {playlist_id}"))?;
    println!("resp {:?}", resp);
    Ok(())
}

/// Change videos description text.
/// In order to update only the `snippet.description` we need to fetch the full video snippet data
/// and replace the description before issuing the update command, otherwise e.g. missing
/// `snippet.tags` info would be reset to default! To update a snippet title and category_id are
/// mandatory
pub(crate) async fn change_description(
    cl: &Hub,
    video_id: &str,
    description: &str,
    change_mode: ChangeMode,
) -> Result<(), YoutubeError> {
    let part = vec!["snippet".to_string()];
    let (_, response) = cl
        .videos()
        .list(&part)
        .add_id(video_id)
        .add_scopes(SCOPES)
        .doit()
        .await
        .or_raise_api(|| format!("fetching video {video_id}"))?;
    let Some(video) = response.items.unwrap_or_default().into_iter().next() else {
        bail!(YoutubeError::not_found(format!(
            "changing description of video {video_id}"
        )));
    };
    let mut vsnip = video.snippet.ok_or_raise(|| {
        YoutubeError::unexpected(format!("video {video_id} was returned without a snippet"))
    })?;
    let old_desc = vsnip.description.unwrap_or_default();
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
    let (_, resp) = cl
        .videos()
        .update(video)
        .add_scopes(SCOPES)
        .doit()
        .await
        .or_raise_api(|| format!("updating description of video {video_id}"))?;
    println!("resp {:?}", resp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_youtube3::Error;

    #[test]
    fn test_parse_publish_datetime() {
        let d = parse_publish_datetime("2026-09-04T08:00:00Z").unwrap();
        assert_eq!(d.to_rfc3339(), "2026-09-04T08:00:00+00:00");
        // Space separated variant as produced by NaiveDateTime Debug formatting
        let d = parse_publish_datetime("2026-09-04 08:00:00Z").unwrap();
        assert_eq!(d.to_rfc3339(), "2026-09-04T08:00:00+00:00");
        assert!(parse_publish_datetime("nonsense").is_err());
    }

    fn bad_request(code: u64, reason: &str) -> Error {
        Error::BadRequest(serde_json::json!({
            "error": { "code": code, "errors": [{ "reason": reason }] }
        }))
    }

    #[test]
    fn test_classify_api_errors() {
        use YoutubeErrorKind::*;
        let kind = |err| YoutubeErrorKind::from_api(&err);
        assert_eq!(kind(bad_request(403, "quotaExceeded")), QuotaExceeded);
        assert_eq!(kind(bad_request(403, "dailyLimitExceeded")), QuotaExceeded);
        assert_eq!(kind(bad_request(403, "forbidden")), Api);
        assert_eq!(kind(bad_request(401, "authError")), Unauthorized);
        assert_eq!(kind(bad_request(404, "videoNotFound")), NotFound);
        assert_eq!(kind(bad_request(429, "rateLimitExceeded")), Temporary);
        assert_eq!(kind(bad_request(503, "backendError")), Temporary);
        assert_eq!(kind(Error::Io(std::io::Error::other("reset"))), Temporary);
        assert_eq!(kind(Error::Cancelled), Api);
    }

    #[test]
    fn test_or_raise_api_keeps_kind_and_source() {
        let result: std::result::Result<(), Error> = Err(bad_request(404, "videoNotFound"));
        let err = result.or_raise_api(|| "fetching video abc").unwrap_err();
        assert_eq!(err.kind(), YoutubeErrorKind::NotFound);
        assert_eq!(err.to_string(), "fetching video abc: not found");
        assert_eq!(err.frame().children().len(), 1);
    }
}
