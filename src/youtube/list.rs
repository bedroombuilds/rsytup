//! Helper functions for listing all uploaded videos
use std::collections::HashMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use futures_util::future;
use serde::Serialize;

/// Maximum page size the `playlistItems` endpoint accepts.
const PAGE_SIZE: u32 = 50;

/// Maximum number of ids the videos endpoint accepts in one call.
const ID_BATCH: usize = 50;

/// Minimal YouTube video information
#[derive(Serialize)]
pub struct YtVideoInfo {
    pub id: String,
    pub title: String,
    pub published_at: Option<DateTime<Utc>>,
    pub privacy_status: Option<String>,
    pub url: String,
    pub duration: Option<String>,
    pub description: Option<String>,
}

impl YtVideoInfo {
    /// Attention: do not use that in a loop, API call quota is limited
    pub async fn from_id(cl: &super::Hub, video_id: &str) -> anyhow::Result<Self> {
        let part = vec!["snippet".into(), "contentDetails".into(), "status".into()];
        let resp = cl
            .videos()
            .list(&part)
            .add_id(video_id)
            .add_scopes(super::SCOPES)
            .doit()
            .await?;
        if let Some(video) = resp.1.items.as_ref().unwrap().iter().take(1).next() {
            let privacy_status = video
                .status
                .as_ref()
                .and_then(|status| status.privacy_status.clone());
            let vsnip = video.snippet.as_ref().unwrap().clone();
            let duration = video
                .content_details
                .as_ref()
                .and_then(|details| details.duration.clone());
            Ok(Self {
                id: video_id.to_string(),
                title: vsnip.title.unwrap(),
                description: vsnip.description,
                published_at: vsnip.published_at,
                privacy_status,
                url: format!("https://www.youtube.com/watch?v={video_id}"),
                duration,
            })
        } else {
            anyhow::bail!("video not found")
        }
    }
}

/// Resolve the "uploads" playlist of the authenticated user's channel.
async fn uploads_playlist_id(hub: &super::Hub) -> anyhow::Result<String> {
    let (_, response) = hub
        .channels()
        .list(&vec!["contentDetails".into()])
        .mine(true)
        .add_scopes(super::SCOPES)
        .doit()
        .await
        .context("listing the authenticated user's channels")?;

    let channel = response
        .items
        .unwrap_or_default()
        .into_iter()
        .next()
        .context("the authenticated account has no YouTube channel")?;

    channel
        .content_details
        .and_then(|details| details.related_playlists)
        .and_then(|playlists| playlists.uploads)
        .context("the channel does not expose an uploads playlist")
}

/// List the account's uploaded videos, newest first, at most `limit` of them.
pub async fn list_uploads(
    hub: &super::Hub,
    limit: Option<usize>,
) -> anyhow::Result<Vec<YtVideoInfo>> {
    if limit == Some(0) {
        return Ok(Vec::new());
    }

    let playlist_id = uploads_playlist_id(hub).await?;
    let mut uploads = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let remaining = limit.map(|limit| limit - uploads.len());
        let page_size = match remaining {
            Some(remaining) => PAGE_SIZE.min(remaining as u32),
            None => PAGE_SIZE,
        };

        let mut call = hub
            .playlist_items()
            .list(&vec![
                "snippet".into(),
                "contentDetails".into(),
                "status".into(),
            ])
            .playlist_id(&playlist_id)
            .max_results(page_size)
            .add_scopes(super::SCOPES);
        if let Some(token) = &page_token {
            call = call.page_token(token);
        }

        let (_, response) = call
            .doit()
            .await
            .context("listing items of the uploads playlist")?;

        for item in response.items.unwrap_or_default() {
            let content_details = item.content_details.unwrap_or_default();
            let snippet = item.snippet.unwrap_or_default();

            let Some(id) = content_details.video_id else {
                // A playlist entry without a video id is not a video we can report on.
                continue;
            };

            uploads.push(YtVideoInfo {
                url: format!("https://www.youtube.com/watch?v={id}"),
                id,
                title: snippet.title.unwrap_or_default(),
                published_at: content_details.video_published_at.or(snippet.published_at),
                duration: None, // will be updated by fetch_durations
                description: snippet.description,
                privacy_status: item.status.and_then(|status| status.privacy_status),
            });
        }

        page_token = response.next_page_token;
        let done = page_token.is_none() || limit.is_some_and(|limit| uploads.len() >= limit);
        if done {
            break;
        }
    }

    if let Some(limit) = limit {
        uploads.truncate(limit);
    }

    let ids: Vec<&str> = uploads.iter().map(|upload| upload.id.as_str()).collect();
    let durations = fetch_durations(hub, &ids).await?;
    for upload in &mut uploads {
        upload.duration = durations.get(&upload.id).cloned();
    }

    Ok(uploads)
}
/// Look up the ISO 8601 duration of every id in `ids`.
///
/// `videos.list` bills one quota unit per call no matter how many ids it
/// carries, so batching by the endpoint's 50-id cap keeps a full listing at two
/// units per 50 videos. Paging the playlist is inherently sequential, but these
/// batches are independent, so issuing them together adds roughly a single
/// round-trip regardless of how large the channel is.
async fn fetch_durations(
    hub: &super::Hub,
    ids: &[&str],
) -> anyhow::Result<HashMap<String, String>> {
    let requests = ids.chunks(ID_BATCH).map(|batch| async move {
        let mut call = hub
            .videos()
            .list(&vec!["contentDetails".into()])
            .add_scopes(super::SCOPES);
        for id in batch {
            call = call.add_id(id);
        }
        call.doit()
            .await
            .context("fetching content details of the listed videos")
    });

    let mut durations = HashMap::new();
    for (_, response) in future::try_join_all(requests).await? {
        for video in response.items.unwrap_or_default() {
            // Key off the returned id rather than the request order: the
            // response silently omits videos that are gone or invisible to us,
            // so it can be shorter than the batch that was asked for.
            let Some(id) = video.id else { continue };
            if let Some(duration) = video.content_details.and_then(|details| details.duration) {
                durations.insert(id, duration);
            }
        }
    }
    Ok(durations)
}

/// Render an ISO 8601 duration such as `PT1H2M3S` as `1:02:03`.
///
/// Returns `None` for anything zero-length or non-parsable. The API reports `P0D`
/// for a live stream that has not ended, which has no meaningful length yet.
fn format_duration(iso: &str) -> Option<String> {
    let body = iso.strip_prefix('P')?;
    let (date, time) = body.split_once('T').unwrap_or((body, ""));

    let seconds = sum_segment(date, |unit| match unit {
        'W' => Some(7 * 86_400),
        'D' => Some(86_400),
        _ => None,
    })? + sum_segment(time, |unit| match unit {
        'H' => Some(3_600),
        'M' => Some(60),
        'S' => Some(1),
        _ => None,
    })?;

    if seconds == 0 {
        return None;
    }
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    Some(match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    })
}

/// Total the `<number><unit>` pairs in one half of an ISO 8601 duration.
fn sum_segment(segment: &str, seconds_per_unit: impl Fn(char) -> Option<u64>) -> Option<u64> {
    let mut total: u64 = 0;
    let mut value: u64 = 0;
    let mut pending = false;

    for ch in segment.chars() {
        match ch.to_digit(10) {
            Some(digit) => {
                value = value.checked_mul(10)?.checked_add(digit as u64)?;
                pending = true;
            }
            None => {
                if !pending {
                    return None;
                }
                total = total.checked_add(value.checked_mul(seconds_per_unit(ch)?)?)?;
                value = 0;
                pending = false;
            }
        }
    }

    // A number with no unit after it means the string was malformed.
    (!pending).then_some(total)
}

pub fn print_table(uploads: &[YtVideoInfo]) {
    if uploads.is_empty() {
        println!("No uploaded videos found.");
        return;
    }

    // Formatted up front so the column can be sized to the widest entry.
    let durations: Vec<String> = uploads
        .iter()
        .map(|upload| {
            upload
                .duration
                .as_deref()
                .and_then(format_duration)
                .unwrap_or_else(|| "-".to_string())
        })
        .collect();

    let privacy_width = uploads
        .iter()
        .map(|upload| upload.privacy_status.as_deref().unwrap_or("-").len())
        .max()
        .unwrap_or(1)
        .max("PRIVACY".len());
    let duration_width = durations
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(1)
        .max("DURATION".len());

    println!(
        "{:<10}  {:<11}  {:<privacy_width$}  {:>duration_width$}  {:>8}  TITLE",
        "PUBLISHED", "VIDEO ID", "PRIVACY", "DURATION", "HAS-DESC"
    );
    for (upload, duration) in uploads.iter().zip(&durations) {
        let published = upload
            .published_at
            .map(|at| at.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string());
        let privacy = upload.privacy_status.as_deref().unwrap_or("-");
        println!(
            "{published:<10}  {:<11}  {privacy:<privacy_width$}  {duration:>duration_width$}  {:>8}  {}",
            upload.id,
            upload.description.is_some(),
            upload.title
        );
    }
}

pub fn print_json(uploads: &[YtVideoInfo]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(uploads).context("serializing uploads to JSON")?;
    println!("{json}");
    Ok(())
}
