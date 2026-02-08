use anyhow::{Context, Result};
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::client::atom;
use crate::client::HatenaClient;
use crate::config::Config;
use crate::entry::LocalEntry;

pub fn run(blogs: &[String], no_drafts: bool, only_drafts: bool) -> Result<()> {
    let config = Config::load(None)?;

    let targets: Vec<String> = if blogs.is_empty() {
        config.blogs.keys().cloned().collect()
    } else {
        blogs.to_vec()
    };

    let (tx, rx) = mpsc::channel::<(PathBuf, LocalEntry)>();

    let writer = std::thread::spawn(move || {
        let mut created_dirs: HashSet<PathBuf> = HashSet::new();
        let mut stdout = std::io::BufWriter::new(std::io::stdout());
        let mut stderr = std::io::BufWriter::new(std::io::stderr());
        let mut buf = String::with_capacity(8192);
        for (path, entry) in rx {
            // Fresh check: skip if local file is newer than remote
            if path.exists() {
                if let Ok(remote_time) = OffsetDateTime::parse(&entry.date, &Iso8601::DEFAULT) {
                    if let Ok(local_time) = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .map(OffsetDateTime::from)
                    {
                        if remote_time <= local_time {
                            continue;
                        }
                        let _ = writeln!(
                            stderr,
                            "       fresh remote={} > local={}",
                            entry.date,
                            local_time.format(&Iso8601::DEFAULT).unwrap_or_default()
                        );
                    }
                }
            }

            if let Some(parent) = path.parent() {
                if created_dirs.insert(parent.to_path_buf()) {
                    std::fs::create_dir_all(parent)
                        .context("Failed to create directory")?;
                }
            }
            buf.clear();
            entry.write_to_string(&mut buf);
            std::fs::write(&path, &buf)
                .context("Failed to write entry file")?;
            let _ = writeln!(stdout, "{}", path.display());
            let _ = writeln!(stderr, "       store {}", path.display());
        }
        Ok::<(), anyhow::Error>(())
    });

    for blog_domain in &targets {
        let blog_config = config.get_blog(blog_domain)?;
        let client = HatenaClient::new(blog_domain, blog_config);

        let mut prefetched = None;
        let first_url = client.collection_url();
        let mut next_page_url = Some(first_url);

        while let Some(url) = next_page_url.take() {
            let body = match prefetched.take() {
                Some(handle) => client.await_fetch(handle)?,
                None => client.get_xml(&url)?,
            };
            let feed = atom::parse_feed(&body)?;

            if let Some(ref next) = feed.next_url {
                prefetched = Some(client.spawn_fetch(next.clone()));
            }
            next_page_url = feed.next_url;

            for atom_entry in feed.entries {
                if no_drafts && atom_entry.draft {
                    continue;
                }
                if only_drafts && !atom_entry.draft {
                    continue;
                }

                let entry = LocalEntry::from_atom(atom_entry);
                let path = entry.file_path(
                    &blog_config.local_root,
                    blog_domain,
                    blog_config.omit_domain,
                );
                tx.send((path, entry))
                    .context("Writer thread terminated unexpectedly")?;
            }
        }
    }

    drop(tx);
    writer
        .join()
        .expect("Writer thread panicked")?;

    Ok(())
}
