// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
//! Here we open the browser for the user, making the use of InstalledAppFlow more convenient as
//! nothing has to be copy/pasted. Reason, the browser will open, the user accepts the requested
//! scope by clicking through e.g. the google oauth2, after this is done a local webserver started
//! by InstalledFlowAuthenticator will consume the token coming from the oauth2 server = no copy or
//! paste needed to continue with the operation even when on command-line.
use async_google_apis_common::yup_oauth2::authenticator_delegate::{
    DefaultInstalledFlowDelegate, InstalledFlowDelegate,
};
use std::future::Future;
use std::pin::Pin;

/// async function to be pinned by the `present_user_url` method of the trait
/// we use the existing `DefaultInstalledFlowDelegate::present_user_url` method as a fallback for
/// when the browser did not open for example, the user still see's the URL.
async fn browser_user_url(url: &str, need_code: bool) -> Result<String, String> {
    if webbrowser::open(url).is_ok() {
        println!("webbrowser was successfully opened.");
    }
    let def_delegate = DefaultInstalledFlowDelegate;
    def_delegate.present_user_url(url, need_code).await
}

/// our custom delegate struct we will implement a flow delegate trait for:
/// in this case we will implement the `InstalledFlowDelegated` trait
#[derive(Copy, Clone)]
pub(crate) struct InstalledFlowBrowserDelegate;

/// here we implement only the present_user_url method with the added webbrowser opening
/// the other behaviour of the trait does not need to be changed.
impl InstalledFlowDelegate for InstalledFlowBrowserDelegate {
    /// the actual presenting of URL and browser opening happens in the function defined above here
    /// we only pin it
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(browser_user_url(url, need_code))
    }
}
