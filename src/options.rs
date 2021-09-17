use crate::date_compute;
use std::error::Error;
use std::path::PathBuf;
use structopt::StructOpt;
use strum::VariantNames;
use strum_macros::{EnumString, EnumVariantNames};

#[derive(Debug, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum PublishDate {
    /// compute date of coming weekday, e.g. friday computes the date of next friday
    Coming(String),
    /// add weeks from episode number found in title / given as argument, see first_episode_date
    WeeksFromEpisode,
    /// uses given ISO formatted date
    IsoDate(String),
    /// uses given ISO formatted date and time
    IsoDateTime(String),
}

#[derive(Debug, EnumString, EnumVariantNames, Clone, Copy)]
#[strum(serialize_all = "kebab_case")]
pub enum ChangeMode {
    Append,
    Replace,
    Prepend,
}

pub fn print_publish_date_enum() {
    for m in PublishDate::VARIANTS {
        println!("{}", m);
    }
}

#[derive(Debug, EnumString, EnumVariantNames, Clone, Copy)]
#[strum(serialize_all = "kebab_case")]
pub enum Categories {
    Science = 28,
    People = 22,
    Comedy = 23,
}

/// Parse a single key-value pair from `KEY=VALUE` format
/// if `=` is missing VALUE is assumed to be empty string
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
    U: std::str::FromStr,
    U::Err: Error + 'static,
{
    match s.find('=') {
        Some(pos) => Ok((s[..pos].parse()?, s[pos + 1..].parse()?)),
        None => Ok((s.parse()?, "".parse()?)),
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Rust Youtube uploader",
    about = "helps automating youtube uploads"
)]
pub(crate) struct Options {
    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub(crate) enum Command {
    /// Upload Content to Youtube
    Upload(UploadOptions),
    /// List Videos from Youtube or your account
    List(ListOptions),
    /// Update existing Content
    Update(UpdateOptions),
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Rust Youtube uploader",
    about = "helps automating youtube uploads"
)]
pub(crate) struct UploadOptions {
    /// filename of video to upload
    #[structopt(short, long)]
    pub file: PathBuf,
    /// description of youtube video
    #[structopt(short, long)]
    pub description: String,
    /// title if none given created from filename
    #[structopt(short, long)]
    pub title: Option<String>,
    /// thumbnail file to use (otherwise generated from video)
    #[structopt(long)]
    pub thumbnail: Option<PathBuf>,
    /// auto-create thumbnail from video at this second
    #[structopt(long, default_value = "360")]
    pub thumb_second: usize,
    /// date to publish at, can be computed format <method>=<value>
    /// to see all available methods use `list --publish-methods`
    #[structopt(short, long,
        default_value = "coming=friday",
        case_insensitive = true,
        parse(try_from_str = parse_key_val),
        number_of_values = 1,
        )]
    pub publish_at: (PublishDate, String),
    /// publishing day-time
    #[structopt(short = "T", long, default_value = "08:00:00")]
    pub publish_time: String,
    /// Number of episode (if not in title)
    #[structopt(short, long)]
    pub episode_nr: Option<u8>,
    /// add video to Playlist (if given)
    #[structopt(long)]
    pub playlist_id: Option<String>,
    /// comma separated keywords list
    #[structopt(long, default_value = "rust,tutorial,youtube,upload,rsytup")]
    pub keywords: String,
    /// privacy status
    #[structopt(long, default_value="private", possible_values = &["public", "private", "unlisted"])]
    pub privacy_status: String,
    /// Category
    #[structopt(long, default_value="science", possible_values = &Categories::VARIANTS)]
    pub category: Categories,
    /// Date of First episode
    #[structopt(long, default_value = "2020-09-01")]
    pub first_episode_date: String,
    /// Pretend shows title, date, description and more that would be used and exits
    #[structopt(long)]
    pub pretend: bool,
    /// path to ffmpeg binary
    #[structopt(long, default_value = "ffmpeg")]
    pub ffmpeg_bin: PathBuf,
}

#[derive(Debug, StructOpt)]
pub(crate) struct ListOptions {
    /// List top 5 videos of youtube
    #[structopt(long)]
    pub yt_top5: bool,
    /// List your uploaded videos
    #[structopt(long)]
    pub uploaded: bool,
    /// Shows a list of available methods to compute publish date
    #[structopt(long)]
    pub publish_methods: bool,
}

#[derive(Debug, StructOpt)]
pub(crate) struct UpdateOptions {
    /// video ID, to loop over all videos use "uploaded"
    #[structopt(long)]
    pub video_id: String,
    /// (re-)generates thumbnail from path where the videos are stored
    /// for a given video ID. Matches filenames using the episode_nr in the title.
    /// uploads new thumbnail to youtube
    #[structopt(long)]
    pub generate_thumbnail: Option<PathBuf>,
    /// the description text of all uploaded Videos
    #[structopt(long)]
    pub description: Option<PathBuf>,
    /// the description text of all uploaded Videos
    #[structopt(long, default_value = "append")]
    pub change_desc: ChangeMode,
    /// auto-create thumbnail from video at this second
    #[structopt(long, default_value = "360")]
    pub thumb_second: usize,
    /// add video to playlist with given id
    #[structopt(long)]
    pub add_to_playlist: Option<String>,
}

impl UploadOptions {
    /// Parse a single key-value pair into PublishDate
    pub fn publish_at(&self) -> PublishDate {
        let value: String = self.publish_at.1.clone();
        match &self.publish_at.0 {
            PublishDate::Coming(_) => PublishDate::Coming(value),
            PublishDate::WeeksFromEpisode => PublishDate::WeeksFromEpisode,
            PublishDate::IsoDate(_) => PublishDate::IsoDate(value),
            PublishDate::IsoDateTime(_) => PublishDate::IsoDateTime(value),
        }
    }

    pub fn tags(&self) -> Vec<String> {
        self.keywords.split(',').map(String::from).collect()
    }

    pub fn publish_datetime(&self) -> anyhow::Result<String> {
        let today = chrono::offset::Local::now().naive_local().date();
        match self.publish_at() {
            // TODO: require chrono >= PR release https://github.com/chronotope/chrono/pull/539
            PublishDate::Coming(wd) => Ok(format!(
                "{:?}T{}Z",
                date_compute::coming_weekday(today, wd.parse()?),
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
                date_compute::parse_iso_date(&date)?,
                self.publish_time
            )),
            PublishDate::IsoDateTime(datetime) => Ok(format!(
                "{:?}Z",
                date_compute::parse_iso_datetime(&datetime)?
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
