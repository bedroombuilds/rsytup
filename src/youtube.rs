// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021 Michael Kefeder
//! YouTube API connection and helper functions

mod oauth_flow;
mod youtube_v3_types;
use youtube_v3_types as yt;

use crate::options::{ChangeMode, UploadOptions};
use async_google_apis_common as common;
use std::rc::Rc;

/// Create a new HTTPS client.
fn https_client() -> common::TlsClient {
    let conn = hyper_rustls::HttpsConnector::with_native_roots();
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
