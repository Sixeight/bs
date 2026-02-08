use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::client::atom;
use crate::config::{Config, ResolvedBlogConfig};
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

        let has_edit_url = entry.edit_url.is_some();

        // Detect blog from EditURL or file path
        let (blog_domain, blog_config) = if let Some(ref edit_url) = entry.edit_url {
            let domain = super::fetch::extract_blog_domain(edit_url)?;
            let config = config.get_blog(&domain)?;
            (domain, config)
        } else {
            let abs_path = std::fs::canonicalize(path)
                .with_context(|| format!("Failed to resolve path: {}", path.display()))?;
            let (domain, config) = config.detect_blog_from_path(&abs_path)?;
            (domain.to_string(), config)
        };

        let client = crate::client::HatenaClient::new(&blog_domain, blog_config);

        // Auto-detect CustomPath from file path
        if entry.custom_path.is_none() {
            if let Some(entry_path) = extract_entry_path(path, blog_config, &blog_domain) {
                if !entry::is_likely_given_path(&entry_path) && !is_under_draft_dir(path) {
                    entry.custom_path = Some(entry_path);
                }
            }
        }

        // Set date to now when publishing (--publish)
        if publish {
            let now = OffsetDateTime::now_utc();
            if let Ok(formatted) = now.format(&Iso8601::DEFAULT) {
                entry.date = formatted;
            }
        }

        let result = if has_edit_url {
            let edit_url = entry.edit_url.as_ref().unwrap();

            // Fresh check: only push if local is newer than remote (app:edited)
            let remote_entry = client.get_entry_by_url(edit_url)?;
            if let Ok(remote_time) = OffsetDateTime::parse(&remote_entry.edited, &Iso8601::DEFAULT)
            {
                if let Some(local_time) = entry::local_last_modified(path) {
                    if local_time <= remote_time {
                        skipped += 1;
                        if is_tty {
                            progress::status(
                                &mut spinner,
                                &format!(
                                    "push  {}/{}  {} pushed  {} skipped",
                                    i + 1,
                                    total,
                                    pushed,
                                    skipped
                                ),
                            );
                        }
                        continue;
                    }
                }
            }

            let atom_entry = entry.to_atom_entry();
            let xml = atom::build_entry_xml(&atom_entry);
            client.update_entry(edit_url, &xml)?
        } else {
            // No EditURL → create new entry via POST
            let atom_entry = entry.to_atom_entry();
            let xml = atom::build_entry_xml(&atom_entry);
            client.create_entry(&xml)?
        };

        let result = LocalEntry::from_atom(result);
        let dest = result.file_path(
            &blog_config.local_root,
            &blog_domain,
            blog_config.omit_domain,
        );
        result.save(&dest)?;

        // Delete old file if path changed (e.g., draft → published)
        let canonical_path = std::fs::canonicalize(path).ok();
        let canonical_dest = std::fs::canonicalize(&dest).ok();
        if let (Some(old), Some(new)) = (&canonical_path, &canonical_dest) {
            if old != new {
                let _ = std::fs::remove_file(path);
            }
        }

        pushed += 1;
        if is_tty {
            progress::status(
                &mut spinner,
                &format!(
                    "push  {}/{}  {} pushed  {} skipped",
                    i + 1,
                    total,
                    pushed,
                    skipped
                ),
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
            pushed,
            skipped,
            pushed + skipped
        ));
    }

    Ok(())
}

/// Extract the entry path portion from a file path relative to the blog's local_root.
/// Returns the path after `entry/` prefix, e.g., `"2024/01/01/test"` for
/// `/tmp/blog/example.com/entry/2024/01/01/test.md`.
fn extract_entry_path(
    file_path: &Path,
    config: &ResolvedBlogConfig,
    blog_domain: &str,
) -> Option<String> {
    let abs_path = std::fs::canonicalize(file_path).ok()?;
    let base = if config.omit_domain {
        config.local_root.clone()
    } else {
        config.local_root.join(blog_domain)
    };
    let base = std::fs::canonicalize(&base).ok().unwrap_or(base);
    let relative = abs_path.strip_prefix(&base).ok()?;
    let rel_str = relative.to_str()?;
    let entry_path = rel_str.strip_prefix("entry/")?;
    // Strip .md extension if present
    let entry_path = entry_path.strip_suffix(".md").unwrap_or(entry_path);
    Some(entry_path.to_string())
}

fn is_under_draft_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "_draft")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_under_draft_dir() {
        assert!(is_under_draft_dir(Path::new(
            "/tmp/blog/example.com/entry/_draft/123.md"
        )));
        assert!(!is_under_draft_dir(Path::new(
            "/tmp/blog/example.com/entry/2024/01/01/test.md"
        )));
    }

    #[test]
    fn test_extract_entry_path() {
        let dir = std::env::temp_dir().join("bs_test_push_extract");
        let _ = std::fs::remove_dir_all(&dir);
        let entry_dir = dir.join("example.com").join("entry").join("my-custom-path");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let file = entry_dir.join("post.md");
        std::fs::write(&file, "test").unwrap();

        let config = ResolvedBlogConfig {
            username: "u".to_string(),
            password: "p".to_string(),
            local_root: dir.clone(),
            omit_domain: false,
            owner: None,
        };

        let result = extract_entry_path(&file, &config, "example.com");
        assert_eq!(result, Some("my-custom-path/post".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
