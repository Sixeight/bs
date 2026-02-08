mod client;
mod commands;
mod config;
mod entry;
mod progress;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bs",
    about = "A fast blogsync-compatible CLI for Hatena Blog",
    version,
    propagate_version = true
)]
struct Cli {
    /// Change working directory before executing
    #[arg(short = 'C', global = true, env = "BLOGSYNC_WORKDIR")]
    workdir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download entries from remote
    #[command(alias = "pl")]
    Pull {
        /// Blog IDs to pull (pulls all configured blogs if omitted)
        #[arg(value_name = "BLOG_ID")]
        blogs: Vec<String>,

        /// Skip draft entries
        #[arg(long)]
        no_drafts: bool,

        /// Pull only draft entries
        #[arg(long, conflicts_with = "no_drafts")]
        only_drafts: bool,
    },

    /// Fetch a specific entry from remote by file path
    Fetch {
        /// Entry file paths to fetch
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    /// Push local entries to remote
    Push {
        /// Entry file paths to push
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Publish draft entries
        #[arg(long)]
        publish: bool,
    },

    /// Create a new entry from stdin
    Post {
        /// Target blog ID (e.g., example.hatenablog.com)
        #[arg(required = true, value_name = "BLOG_ID")]
        blog: String,

        /// Entry title
        #[arg(long)]
        title: Option<String>,

        /// Create as draft
        #[arg(long)]
        draft: bool,

        /// Custom URL path
        #[arg(long, value_name = "PATH")]
        custom_path: Option<String>,

        /// Create as a static page
        #[arg(long)]
        page: bool,
    },

    /// List configured blogs
    #[command(alias = "ls")]
    List,

    /// Remove entries
    #[command(alias = "rm")]
    Remove {
        /// Entry file paths to remove
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Some(workdir) = &cli.workdir {
        std::env::set_current_dir(workdir)?;
    }

    match cli.command {
        Commands::Pull {
            blogs,
            no_drafts,
            only_drafts,
        } => commands::pull::run(&blogs, no_drafts, only_drafts),
        Commands::Fetch { paths } => commands::fetch::run(&paths),
        Commands::Push { paths, publish } => commands::push::run(&paths, publish),
        Commands::Post {
            blog,
            title,
            draft,
            custom_path,
            page,
        } => commands::post::run(&blog, title.as_deref(), draft, custom_path.as_deref(), page),
        Commands::List => commands::list::run(),
        Commands::Remove { paths } => commands::remove::run(&paths),
    }
}
