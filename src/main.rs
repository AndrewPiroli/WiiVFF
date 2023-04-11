use clap::{Parser, Subcommand};
use std::{fs::File, path::PathBuf};
use wiivff::{Result, VFF};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
    #[arg(long, global = true)]
    /// Show deleted
    show_deleted: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List the contents of the VFF
    List {
        /// The path to the input file (cdb.vff)
        src: PathBuf,
    },
    /// Dump the VFF to disk
    Dump {
        /// The path to the input file (cdb.vff)
        src: PathBuf,
        /// Path to dump to
        dest: PathBuf,
    },
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    match args.cmd {
        Commands::List { src } => {
            let file = File::open(src)?;
            let (_, root_dir) = VFF::new(file)?;
            for entry in root_dir.ls(args.show_deleted)? {
                println!("{entry}");
            }
        }
        Commands::Dump { src, dest } => {
            let file = File::open(src)?;
            let (_, root_dir) = VFF::new(file)?;
            root_dir.dump(dest, args.show_deleted)?;
        }
    }
    Ok(())
}
