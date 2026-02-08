use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::entry::LocalEntry;
use crate::progress;

pub fn run(paths: &[PathBuf]) -> Result<()> {
    let config = Config::load(None)?;

    let is_tty = progress::stderr_is_tty();
    let mut spinner = progress::Spinner::new();
    let total = paths.len();

    for (i, path) in paths.iter().enumerate() {
        let entry = LocalEntry::from_file(path)?;
        let edit_url = entry
            .edit_url
            .as_ref()
            .context("Entry has no EditURL; cannot remove")?;

        let blog_domain = super::fetch::extract_blog_domain(edit_url)?;
        let blog_config = config.get_blog(&blog_domain)?;

        if is_tty {
            progress::status(
                &mut spinner,
                &format!("remove  {}/{}", i + 1, total),
            );
        }

        let client = crate::client::HatenaClient::new(&blog_domain, blog_config);
        client.delete_entry(edit_url)?;

        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove local file: {}", path.display()))?;
    }

    if is_tty {
        progress::finish(&format!("remove  {} deleted", total));
    }

    Ok(())
}
