use structopt::StructOpt;

mod date_compute;
mod options;

use options::{Command, Options};

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();
    match options.cmd {
        Command::Upload(options) => {
            if options.pretend {
                println!("publish-at: {:?}", options.publish_at());
                println!("publish-datetime: {}", options.publish_datetime()?);
                if let Ok(episode_nr) = options.episode_nr() {
                    println!("episode_nr: {} (0x{:X})", episode_nr, episode_nr);
                    println!(r#"youtube-title: "{:X}. {}""#, episode_nr, options.title());
                } else {
                    println!("episode_nr: n.a.");
                    println!(r#"youtube-title: "{}""#, options.title());
                }
                println!("catgegory: {:?}", options.category);
                println!("thumb-title: {:?}", options.title());
                println!("youtube-description: {}", &options.description);
                println!("youtube-tags: {:?}", &options.tags());
                std::process::exit(0);
            }
        }
        Command::List(options) => {
            if options.publish_methods {
                crate::options::print_publish_date_enum();
                std::process::exit(0);
            }
            if options.yt_top5 {
                eprintln!("Not yet implemented");
                std::process::exit(1);
            }
            if options.uploaded {
                eprintln!("Not yet implemented");
                std::process::exit(1);
            }
        }
        Command::Update(_options) => {
            eprintln!("Not yet implemented");
        }
    }
    Ok(())
}
