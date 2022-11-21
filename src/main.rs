use std::fs::File;
use wiivff::{VFF, Result};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    src: String,
    #[arg(short, long, value_name = "OUTPUT DIR")]
    dump: Option<String>,
    #[arg(long)]
    show_deleted: bool,
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    let file = File::open(args.src)?;
    let dumping = args.dump.is_some();
    let (_, root_dir) = VFF::new(file)?;
    let res = root_dir.ls(None, args.dump, args.show_deleted)?;
    if !dumping {
        eprintln!("Directory Listing:");
        for entry in res {
            println!("{entry}");
        }
    }
    Ok(())
}
