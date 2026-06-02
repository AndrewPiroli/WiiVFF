use clap::{Parser, Subcommand};
use std::{fs::File, path::PathBuf};
use wiivff::{build_vff, Result, BUILD_DEFAULT_VOLUME_SIZE, VFF};

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
    /// Build a directory into a new VFF file
    Build {
        /// The source directory to pack
        src: PathBuf,
        /// The path to write the output VFF file to
        dest: PathBuf,
        /// Total volume size in bytes (must be a multiple of 0x200).
        /// Defaults to 0x1400000 (20 MiB), matching a standard Wii VFF.
        #[arg(long, default_value_t = BUILD_DEFAULT_VOLUME_SIZE)]
        volume_size: u32,
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
        Commands::Build {
            src,
            dest,
            volume_size,
        } => {
            build_vff(&src, &dest, volume_size)?;
        }
    }
    Ok(())
}
