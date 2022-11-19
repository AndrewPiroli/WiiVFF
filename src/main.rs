use std::fs::File;
use wiivff::{VFF, VFFError, Result};

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    src: String,
    #[arg(short, long, value_name = "OUTPUT DIR")]
    dump: Option<String>
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    if args.dump.is_some() {
        return Err(VFFError::Other("Dumping is not supported.".to_owned()));
    }
    let file = File::open(args.src)?;
    let (_, root_dir) = VFF::new(file)?;
    let ls_res = root_dir.ls(None)?;
    eprintln!("Directory Listing:");
    for ls_entry in ls_res {
        println!("{ls_entry}");
    }
    Ok(())
}