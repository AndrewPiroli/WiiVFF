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

    let (_, root_dir) = VFF::new(file)?;
    if let Some(dump_location) = args.dump {
        root_dir.dump(dump_location, args.show_deleted)?;
    }
    else {
        eprintln!("Directory Listing:");
        for entry in root_dir.ls(args.show_deleted)? {
            println!("{entry}");
        }
    }
    Ok(())
}
