use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::entry::LocalEntry;

pub fn run(paths: &[PathBuf]) -> Result<()> {
    let config = Config::load(None)?;

    for path in paths {
        let entry = LocalEntry::from_file(path)?;
        let edit_url = entry
            .edit_url
            .as_ref()
            .context("Entry has no EditURL; cannot remove")?;

        let blog_domain = super::fetch::extract_blog_domain(edit_url)?;
        let blog_config = config.get_blog(&blog_domain)?;

        let client = crate::client::HatenaClient::new(&blog_domain, blog_config);
        client.delete_entry(edit_url)?;

        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove local file: {}", path.display()))?;
        println!("{}", path.display());
        eprintln!("      remove {}", path.display());
    }

    Ok(())
}
