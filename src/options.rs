//! Command line options and configuration settings for rsytup
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use std::error::Error;
use std::path::PathBuf;

use strum::{EnumMessage, IntoEnumIterator};

use crate::date_compute;
#[derive(Debug, Clone, strum::EnumIter, strum::EnumMessage)]
#[strum(serialize_all = "kebab_case")]
pub enum PublishDate {
    /// Current date at 0 o'clock, publishes therefore as soon as possible
    Asap,
    /// Compute date of coming weekday, e.g. friday computes the date of next friday
    Coming(String),
    /// Add weeks from episode number found in title / given as argument, see first_episode_date
    WeeksFromEpisode,
    /// Uses given ISO formatted date
    IsoDate(String),
    /// Uses given ISO formatted date and time
    IsoDateTime(String),
}

impl std::str::FromStr for PublishDate {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (k, v) = parse_key_val::<String, String>(s)?;
        match k.as_str() {
            "asap" => Ok(PublishDate::Asap),
            "coming" => Ok(PublishDate::Coming(v)),
            "weeks-from-episode" => Ok(PublishDate::WeeksFromEpisode),
            "iso-date" => Ok(PublishDate::IsoDate(v)),
            "iso-date-time" => Ok(PublishDate::IsoDateTime(v)),
            _ => anyhow::bail!("variant not found"),
        }
    }
}

#[derive(Debug, clap::ValueEnum, Clone, Copy)]
#[clap(rename_all = "kebab_case")]
pub enum ChangeMode {
    Append,
    Replace,
    Prepend,
}

#[derive(Debug, clap::ValueEnum, Clone, Copy, strum::Display)]
#[clap(rename_all = "kebab_case")]
#[strum(serialize_all = "kebab_case")]
pub enum PrivacyStates {
    Public,
    Private,
    Unlisted,
}

pub fn print_publish_date_enum() {
    for m in PublishDate::iter() {
        println!(
            "{:?} {}",
            m.get_serializations(),
            m.get_documentation().unwrap()
        );
    }
}

#[derive(Debug, clap::ValueEnum, Clone, Copy)]
#[clap(rename_all = "kebab_case")]
pub enum Categories {
    Science = 28,
    People = 22,
    Comedy = 23,
}

/// Parse a single key-value pair from `KEY=VALUE` format
/// if `=` is missing VALUE is assumed to be empty string
fn parse_key_val<T, U>(s: &str) -> anyhow::Result<(T, U)>
where
    T: std::str::FromStr,
    T::Err: Error + 'static + Send + Sync,
    U: std::str::FromStr,
    U::Err: Error + 'static + Send + Sync,
{
    match s.find('=') {
        Some(pos) => Ok((s[..pos].parse()?, s[pos + 1..].parse()?)),
        None => Ok((s.parse()?, "".parse()?)),
    }
}

#[derive(Debug, clap::Parser)]
#[clap(
    name = "Rust YouTube uploader",
    about = "helps automating YouTube uploads"
)]
pub(crate) struct Options {
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum Command {
    /// Upload Content to YouTube
    Upload(UploadOptions),
    /// List Videos from YouTube or your account
    List(ListOptions),
    /// Update existing Content
    Update(UpdateOptions),
}

#[derive(Debug, clap::Parser)]
pub(crate) struct UploadOptions {
    /// Filename of video to upload
    #[clap(short, long)]
    pub file: PathBuf,
    /// Description of YouTube video
    #[clap(short, long)]
    pub description: String,
    /// Title if none given created from filename
    #[clap(short, long)]
    pub title: Option<String>,
    /// Thumbnail file to use (otherwise generated from video)
    #[clap(long)]
    pub thumbnail: Option<PathBuf>,
    /// Thumbnail watermark file to use, will be placed on top of screenshot
    #[clap(long, default_value = "logos.png")]
    pub thumbnail_watermark: PathBuf,
    /// Auto-create thumbnail from video at this second
    #[clap(long, default_value = "360")]
    pub thumb_second: usize,
    /// Date to publish at, can be computed format <method>=<value>
    /// to see all available methods use `list --publish-methods`
    #[clap(short, long, default_value = "coming=friday", number_of_values = 1)]
    pub publish_at: PublishDate,
    /// Publishing day-time
    #[clap(short = 'T', long, default_value = "08:00:00")]
    pub publish_time: String,
    /// Number of episode (if not in title)
    #[clap(short, long)]
    pub episode_nr: Option<u8>,
    /// Add video to Playlist (if given)
    #[clap(long)]
    pub playlist_id: Option<String>,
    /// Comma separated keywords list
    #[clap(long, default_value = "rust,tutorial,YouTube,upload,rsytup")]
    pub keywords: String,
    /// Privacy status
    #[clap(long, default_value = "private")]
    pub privacy_status: PrivacyStates,
    /// Category
    #[clap(long, default_value = "science")]
    pub category: Categories,
    /// Date of First episode
    #[clap(long, default_value = "2020-09-01")]
    pub first_episode_date: String,
    /// Pretend shows title, date, description, and more, that would be used and exits
    #[clap(long)]
    pub pretend: bool,
    /// Path to `ffmpeg` binary
    #[clap(long, default_value = "ffmpeg")]
    pub ffmpeg_bin: PathBuf,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct ListOptions {
    /// List top 5 videos of YouTube
    #[clap(long)]
    pub yt_top5: bool,
    /// List your uploaded videos
    #[clap(long)]
    pub uploaded: bool,
    /// Shows a list of available methods to compute publish date
    #[clap(long)]
    pub publish_methods: bool,
    /// Format output as JSON
    #[clap(long)]
    pub json: bool,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct UpdateOptions {
    /// Video ID, to loop overall videos use "uploaded"
    #[clap(long)]
    pub video_id: String,
    /// (re-)generates thumbnail from path where the videos are stored
    /// for a given video ID. Matches filenames using the episode_nr in the title.
    /// Uploads new thumbnail to YouTube
    #[clap(long)]
    pub generate_thumbnail: Option<PathBuf>,
    /// Thumbnail watermark file to use, will be placed on top of screenshot
    #[clap(long, default_value = "logos.png")]
    pub thumbnail_watermark: PathBuf,
    /// The description text of all uploaded Videos
    #[clap(long)]
    pub description: Option<PathBuf>,
    /// The description text of all uploaded Videos
    #[clap(long, default_value = "append")]
    pub change_desc: ChangeMode,
    /// Auto-create thumbnail from video at this second
    #[clap(long, default_value = "360")]
    pub thumb_second: usize,
    /// Add video to playlist with given id
    #[clap(long)]
    pub add_to_playlist: Option<String>,
    /// Path to `ffmpeg` binary
    #[clap(long, default_value = "ffmpeg")]
    pub ffmpeg_bin: PathBuf,
}

impl UploadOptions {
    pub fn tags(&self) -> Vec<String> {
        self.keywords.split(',').map(String::from).collect()
    }

    pub fn publish_datetime(&self) -> anyhow::Result<String> {
        let today = chrono::offset::Local::now().naive_local().date();
        match &self.publish_at {
            PublishDate::Asap => Ok(format!("{:?}T00:00:00Z", today)),
            PublishDate::Coming(wd) => Ok(format!(
                "{:?}T{}Z",
                date_compute::coming_weekday(today, wd.to_owned().parse()?),
                self.publish_time
            )),
            PublishDate::WeeksFromEpisode => Ok(format!(
                "{:?}T{}Z",
                date_compute::add_weeks(
                    date_compute::parse_iso_date(&self.first_episode_date)?,
                    self.episode_nr()?,
                ),
                self.publish_time
            )),
            PublishDate::IsoDate(date) => Ok(format!(
                "{:?}T{}Z",
                date_compute::parse_iso_date(date)?,
                self.publish_time
            )),
            PublishDate::IsoDateTime(datetime) => Ok(format!(
                "{:?}Z",
                date_compute::parse_iso_datetime(datetime)?
            )),
        }
    }

    /// If title is given use it, otherwise create from filename
    pub fn title(&self) -> String {
        if self.title.is_some() {
            self.title.as_deref().unwrap().to_string()
        } else {
            self.file
                .clone()
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        }
    }

    /// If episode_nr is given use it, otherwise
    /// try to convert first two chars in title from hex to `u8`
    pub fn episode_nr(&self) -> anyhow::Result<u8> {
        if let Some(episode_nr) = self.episode_nr {
            Ok(episode_nr)
        } else {
            let number = self.title().chars().take(2).collect::<String>();
            std::primitive::u8::from_str_radix(&number, 16)
                .map_err(|_| anyhow::anyhow!("first two digits of title should be hex-number."))
        }
    }
}
