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
    /// current date at 0 o'clock, publishes therefore as soon as possible
    Asap,
    /// compute date of coming weekday, e.g. friday computes the date of next friday
    Coming(String),
    /// add weeks from episode number found in title / given as argument, see first_episode_date
    WeeksFromEpisode,
    /// uses given ISO formatted date
    IsoDate(String),
    /// uses given ISO formatted date and time
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
    name = "Rust Youtube uploader",
    about = "helps automating youtube uploads"
)]
pub(crate) struct Options {
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum Command {
    /// Upload Content to Youtube
    Upload(UploadOptions),
    /// List Videos from Youtube or your account
    List(ListOptions),
    /// Update existing Content
    Update(UpdateOptions),
}

#[derive(Debug, clap::Parser)]
pub(crate) struct UploadOptions {
    /// filename of video to upload
    #[clap(short, long)]
    pub file: PathBuf,
    /// description of youtube video
    #[clap(short, long)]
    pub description: String,
    /// title if none given created from filename
    #[clap(short, long)]
    pub title: Option<String>,
    /// thumbnail file to use (otherwise generated from video)
    #[clap(long)]
    pub thumbnail: Option<PathBuf>,
    /// thumbnail watermark file to use, will be placed ontop of screenshot
    #[clap(long, default_value = "logos.png")]
    pub thumbnail_watermark: PathBuf,
    /// auto-create thumbnail from video at this second
    #[clap(long, default_value = "360")]
    pub thumb_second: usize,
    /// date to publish at, can be computed format <method>=<value>
    /// to see all available methods use `list --publish-methods`
    #[clap(short, long, default_value = "coming=friday", number_of_values = 1)]
    pub publish_at: PublishDate,
    /// publishing day-time
    #[clap(short = 'T', long, default_value = "08:00:00")]
    pub publish_time: String,
    /// Number of episode (if not in title)
    #[clap(short, long)]
    pub episode_nr: Option<u8>,
    /// add video to Playlist (if given)
    #[clap(long)]
    pub playlist_id: Option<String>,
    /// comma separated keywords list
    #[clap(long, default_value = "rust,tutorial,youtube,upload,rsytup")]
    pub keywords: String,
    /// privacy status
    #[clap(long, default_value = "private")]
    pub privacy_status: PrivacyStates,
    /// Category
    #[clap(long, default_value = "science")]
    pub category: Categories,
    /// Date of First episode
    #[clap(long, default_value = "2020-09-01")]
    pub first_episode_date: String,
    /// Pretend shows title, date, description and more that would be used and exits
    #[clap(long)]
    pub pretend: bool,
    /// path to ffmpeg binary
    #[clap(long, default_value = "ffmpeg")]
    pub ffmpeg_bin: PathBuf,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct ListOptions {
    /// List top 5 videos of youtube
    #[clap(long)]
    pub yt_top5: bool,
    /// List your uploaded videos
    #[clap(long)]
    pub uploaded: bool,
    /// Shows a list of available methods to compute publish date
    #[clap(long)]
    pub publish_methods: bool,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct UpdateOptions {
    /// video ID, to loop over all videos use "uploaded"
    #[clap(long)]
    pub video_id: String,
    /// (re-)generates thumbnail from path where the videos are stored
    /// for a given video ID. Matches filenames using the episode_nr in the title.
    /// uploads new thumbnail to youtube
    #[clap(long)]
    pub generate_thumbnail: Option<PathBuf>,
    /// thumbnail watermark file to use, will be placed ontop of screenshot
    #[clap(long, default_value = "logos.png")]
    pub thumbnail_watermark: PathBuf,
    /// the description text of all uploaded Videos
    #[clap(long)]
    pub description: Option<PathBuf>,
    /// the description text of all uploaded Videos
    #[clap(long, default_value = "append")]
    pub change_desc: ChangeMode,
    /// auto-create thumbnail from video at this second
    #[clap(long, default_value = "360")]
    pub thumb_second: usize,
    /// add video to playlist with given id
    #[clap(long)]
    pub add_to_playlist: Option<String>,
    /// path to ffmpeg binary
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

    /// if title is given use it, otherwise create from filename
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

    /// if episode_nr is given use it, otherwise
    /// try to convert first two chars in title from hex to u8
    pub fn episode_nr(&self) -> anyhow::Result<u8> {
        if self.episode_nr.is_some() {
            Ok(self.episode_nr.unwrap())
        } else {
            let number = self.title().chars().take(2).collect::<String>();
            std::primitive::u8::from_str_radix(&number, 16)
                .map_err(|_| anyhow::anyhow!("first two digits of title should be hex-number."))
        }
    }
}
