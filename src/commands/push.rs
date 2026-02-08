use anyhow::{Context, Result};
use std::path::PathBuf;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::client::atom;
use crate::config::Config;
use crate::entry::LocalEntry;

pub fn run(paths: &[PathBuf], publish: bool) -> Result<()> {
    let config = Config::load(None)?;

    for path in paths {
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

        // Fresh check: only push if local is newer than remote
        let remote_entry = client.get_entry_by_url(edit_url)?;
        if let Ok(remote_time) = OffsetDateTime::parse(&remote_entry.updated, &Iso8601::DEFAULT) {
            if let Ok(local_time) = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .map(OffsetDateTime::from)
            {
                if local_time <= remote_time {
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
        println!("{}", dest.display());
        eprintln!("       store {}", dest.display());
    }

    Ok(())
}
