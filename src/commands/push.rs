use anyhow::{Context, Result};
use std::path::PathBuf;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::client::atom;
use crate::config::Config;
use crate::entry::{self, LocalEntry};
use crate::progress;

pub fn run(paths: &[PathBuf], publish: bool) -> Result<()> {
    let config = Config::load(None)?;

    let is_tty = progress::stderr_is_tty();
    let stdout_tty = progress::stdout_is_tty();
    let mut spinner = progress::Spinner::new();
    let total = paths.len();
    let mut pushed = 0usize;
    let mut skipped = 0usize;

    for (i, path) in paths.iter().enumerate() {
        let mut entry = LocalEntry::from_file(path)?;

        if publish {
            entry.draft = false;
        }

        let edit_url = entry
            .edit_url
            .as_ref()
            .context("Entry has no EditURL; cannot push")?;

        let blog_domain = super::fetch::extract_blog_domain(edit_url)?;
        let blog_config = config.get_blog(&blog_domain)?;

        let client = crate::client::HatenaClient::new(&blog_domain, blog_config);

        // Fresh check: only push if local is newer than remote (app:edited)
        let remote_entry = client.get_entry_by_url(edit_url)?;
        if let Ok(remote_time) = OffsetDateTime::parse(&remote_entry.edited, &Iso8601::DEFAULT) {
            if let Some(local_time) = entry::local_last_modified(path) {
                if local_time <= remote_time {
                    skipped += 1;
                    if is_tty {
                        progress::status(
                            &mut spinner,
                            &format!("push  {}/{}  {} pushed  {} skipped", i + 1, total, pushed, skipped),
                        );
                    }
                    continue;
                }
            }
        }

        let atom_entry = entry.to_atom_entry();
        let xml = atom::build_entry_xml(&atom_entry);
        let updated = client.update_entry(edit_url, &xml)?;

        let result = LocalEntry::from_atom(updated);
        let dest = result.file_path(
            &blog_config.local_root,
            &blog_domain,
            blog_config.omit_domain,
        );
        result.save(&dest)?;

        pushed += 1;
        if is_tty {
            progress::status(
                &mut spinner,
                &format!("push  {}/{}  {} pushed  {} skipped", i + 1, total, pushed, skipped),
            );
        } else {
            progress::log_store(&dest);
        }
        if !stdout_tty {
            println!("{}", dest.display());
        }
    }

    if is_tty {
        progress::finish(&format!(
            "push  {} pushed  {} skipped  {} total",
            pushed, skipped, pushed + skipped
        ));
    }

    Ok(())
}
