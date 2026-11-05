//! Command line options and configuration settings for rsytup
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use std::path::PathBuf;

use thiserror::Error;

use strum::{EnumMessage, IntoEnumIterator};

use crate::date_compute;

/// What went wrong while interpreting `rsytup`'s command line options.
///
/// Each variant carries the context a human needs to fix the offending option
/// or value. These are all "fix the input" errors; nothing is retryable.
#[derive(Debug, Error)]
pub enum OptionsError {
    /// The `publish-at` method is not one of the known `PublishDate` variants.
    #[error(
        "unknown publish method `{method}`, expected one of: {}",
        publish_methods()
    )]
    UnknownPublishMethod {
        /// The offending method name, without its `=VALUE` part.
        method: String,
    },

    /// A weekday name (e.g. in `coming=friday`) is not a valid `Weekday`.
    #[error("could not parse weekday `{weekday}`: {source}")]
    ParseWeekday {
        /// The offending weekday name.
        weekday: String,
        /// The underlying parse error from `FromStr`.
        source: chrono::ParseWeekdayError,
    },

    /// A date / date-time in `iso-date` or `iso-date-time` is not ISO 8601.
    #[error("could not parse date `{date}`: {source}")]
    ParseDate {
        /// The offending date / date-time value.
        date: String,
        /// The underlying date parse error.
        source: chrono::ParseError,
    },

    /// `--file` has no file name (e.g. `..` or `/`) to derive a title from.
    #[error("`{}` has no file name to derive a title from", file.display())]
    NoFileName {
        /// The offending path.
        file: PathBuf,
    },

    /// The first two characters of the title are not a hex number.
    #[error("first two digits of title `{title}` should be hex-number.")]
    EpisodeNrNotHex {
        /// The title the episode number was derived from.
        title: String,
    },
}

#[derive(Debug, Clone, strum::EnumIter, strum::EnumMessage)]
#[strum(serialize_all = "kebab_case")]
pub enum PublishDate {
    /// Current date at 0 o'clock, publishes therefore as soon as possible
    Asap,
    /// Compute date of coming weekday, e.g. Friday computes the date of next Friday
    Coming(String),
    /// Add weeks from episode number found in title / given as argument, see first_episode_date
    WeeksFromEpisode,
    /// Uses given ISO formatted date
    IsoDate(String),
    /// Uses given ISO formatted date and time
    IsoDateTime(String),
}

impl std::str::FromStr for PublishDate {
    type Err = OptionsError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // `KEY=VALUE`; if `=` is missing VALUE is assumed to be empty
        let (k, v) = s.split_once('=').unwrap_or((s, ""));
        let v = v.to_string();
        match k {
            "asap" => Ok(PublishDate::Asap),
            "coming" => Ok(PublishDate::Coming(v)),
            "weeks-from-episode" => Ok(PublishDate::WeeksFromEpisode),
            "iso-date" => Ok(PublishDate::IsoDate(v)),
            "iso-date-time" => Ok(PublishDate::IsoDateTime(v)),
            _ => Err(OptionsError::UnknownPublishMethod {
                method: k.to_string(),
            }),
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

/// The valid `publish-at` method names, comma separated.
fn publish_methods() -> String {
    PublishDate::iter()
        .flat_map(|m| m.get_serializations())
        .copied()
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn print_publish_date_enum() {
    for m in PublishDate::iter() {
        println!(
            "{:?} {}",
            m.get_serializations(),
            m.get_documentation()
                .expect("every PublishDate variant has a doc comment")
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

/// Parse `--file`, which needs a file name to derive the default title from.
fn parse_video_file(s: &str) -> Result<PathBuf, OptionsError> {
    let file = PathBuf::from(s);
    if file.file_stem().is_none() {
        return Err(OptionsError::NoFileName { file });
    }
    Ok(file)
}

#[derive(Debug, clap::Parser)]
pub(crate) struct UploadOptions {
    /// Filename of video to upload
    #[clap(short, long, value_parser = parse_video_file)]
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
    /// Details about one video-id
    #[clap(long)]
    pub video_id: Option<String>,
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

    pub fn publish_datetime(&self) -> Result<String, OptionsError> {
        let today = chrono::offset::Local::now().naive_local().date();
        match &self.publish_at {
            PublishDate::Asap => Ok(format!("{:?}T00:00:00Z", today)),
            PublishDate::Coming(wd) => {
                let weekday = wd.parse().map_err(|source| OptionsError::ParseWeekday {
                    weekday: wd.clone(),
                    source,
                })?;
                Ok(format!(
                    "{:?}T{}Z",
                    date_compute::coming_weekday(today, weekday),
                    self.publish_time
                ))
            }
            PublishDate::WeeksFromEpisode => Ok(format!(
                "{:?}T{}Z",
                date_compute::add_weeks(
                    self.parse_iso_date(&self.first_episode_date)?,
                    self.episode_nr()?,
                ),
                self.publish_time
            )),
            PublishDate::IsoDate(date) => Ok(format!(
                "{:?}T{}Z",
                self.parse_iso_date(date)?,
                self.publish_time
            )),
            PublishDate::IsoDateTime(datetime) => Ok(format!(
                "{:?}Z",
                date_compute::parse_iso_datetime(datetime).map_err(|source| {
                    OptionsError::ParseDate {
                        date: datetime.clone(),
                        source,
                    }
                })?
            )),
        }
    }

    /// Parse an ISO 8601 date, reporting which value failed to parse.
    fn parse_iso_date(&self, date: &str) -> Result<chrono::NaiveDate, OptionsError> {
        date_compute::parse_iso_date(date).map_err(|source| OptionsError::ParseDate {
            date: date.to_string(),
            source,
        })
    }

    /// If title is given use it, otherwise create from filename
    pub fn title(&self) -> String {
        match &self.title {
            Some(title) => title.clone(),
            None => self
                .file
                .file_stem()
                .expect("--file is validated by parse_video_file to have a file name")
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// If episode_nr is given use it, otherwise
    /// try to convert first two chars in title from hex to `u8`
    pub fn episode_nr(&self) -> Result<u8, OptionsError> {
        if let Some(episode_nr) = self.episode_nr {
            Ok(episode_nr)
        } else {
            let number = self.title().chars().take(2).collect::<String>();
            std::primitive::u8::from_str_radix(&number, 16).map_err(|_| {
                OptionsError::EpisodeNrNotHex {
                    title: self.title(),
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_publish_method_names_input_and_choices() {
        let err = "bogus=1".parse::<PublishDate>().unwrap_err().to_string();
        assert!(err.contains("`bogus`"), "{err}");
        assert!(err.contains("asap"), "{err}");
        assert!(err.contains("iso-date-time"), "{err}");
    }

    #[test]
    fn test_every_publish_method_is_documented() {
        for m in PublishDate::iter() {
            assert!(m.get_documentation().is_some(), "{m:?} has no doc comment");
        }
    }

    #[test]
    fn test_parse_video_file() {
        assert!(parse_video_file("..").is_err());
        assert!(parse_video_file("/").is_err());
        assert_eq!(
            parse_video_file("01.intro.mov").unwrap(),
            PathBuf::from("01.intro.mov")
        );
    }

    #[test]
    fn test_publish_date_key_value() {
        assert!(matches!("asap".parse(), Ok(PublishDate::Asap)));
        assert!(matches!("coming=friday".parse(), Ok(PublishDate::Coming(day)) if day == "friday"));
    }
}
