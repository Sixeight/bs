use anyhow::{Context, Result};
use std::path::PathBuf;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::config::Config;
use crate::entry::{self, LocalEntry};
use crate::progress;

pub fn run(paths: &[PathBuf]) -> Result<()> {
    let config = Config::load(None)?;

    let is_tty = progress::stderr_is_tty();
    let stdout_tty = progress::stdout_is_tty();
    let mut spinner = progress::Spinner::new();
    let total = paths.len();
    let mut stored = 0usize;
    let mut unchanged = 0usize;

    for (i, path) in paths.iter().enumerate() {
        let entry = LocalEntry::from_file(path)?;
        let edit_url = entry
            .edit_url
            .as_ref()
            .context("Entry has no EditURL; cannot fetch")?;

        let blog_domain = extract_blog_domain(edit_url)?;
        let blog_config = config.get_blog(&blog_domain)?;

        let client = crate::client::HatenaClient::new(&blog_domain, blog_config);
        let atom_entry = client.get_entry_by_url(edit_url)?;
        let updated = LocalEntry::from_atom(atom_entry);
        let dest = updated.file_path(
            &blog_config.local_root,
            &blog_domain,
            blog_config.omit_domain,
        );

        let local_time = entry::local_last_modified(&dest);
        if let Ok(remote_time) = OffsetDateTime::parse(&updated.date, &Iso8601::DEFAULT) {
            if let Some(lt) = local_time {
                if !remote_time.gt(&lt) {
                    unchanged += 1;
                    if is_tty {
                        progress::status(
                            &mut spinner,
                            &format!("fetch  {}/{}  {} stored  {} unchanged", i + 1, total, stored, unchanged),
                        );
                    }
                    continue;
                }
            }
            let local_str = local_time
                .and_then(|t| t.format(&Iso8601::DEFAULT).ok())
                .unwrap_or_default();
            progress::log_fresh(&updated.date, &local_str);
        }

        updated.save(&dest)?;

        stored += 1;
        if is_tty {
            progress::status(
                &mut spinner,
                &format!("fetch  {}/{}  {} stored  {} unchanged", i + 1, total, stored, unchanged),
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
            "fetch  {} stored  {} unchanged",
            stored, unchanged
        ));
    }

    Ok(())
}

pub fn extract_blog_domain(edit_url: &str) -> Result<String> {
    // EditURL format: https://blog.hatena.ne.jp/{owner}/{blog_domain}/atom/entry/{id}
    let path = edit_url
        .find("://")
        .and_then(|i| edit_url[i + 3..].find('/'))
        .map(|i| &edit_url[edit_url.find("://").unwrap() + 3 + i + 1..])
        .context("Invalid EditURL")?;
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() >= 2 {
        Ok(segments[1].to_string())
    } else {
        anyhow::bail!("Cannot extract blog domain from EditURL: {}", edit_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_blog_domain_valid() {
        let url = "https://blog.hatena.ne.jp/user/example.hatenablog.com/atom/entry/123";
        let domain = extract_blog_domain(url).unwrap();
        assert_eq!(domain, "example.hatenablog.com");
    }

    #[test]
    fn test_extract_blog_domain_hateblo() {
        let url = "https://blog.hatena.ne.jp/someone/myblog.hateblo.jp/atom/entry/456";
        let domain = extract_blog_domain(url).unwrap();
        assert_eq!(domain, "myblog.hateblo.jp");
    }

    #[test]
    fn test_extract_blog_domain_invalid_url() {
        let result = extract_blog_domain("not a url");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_blog_domain_too_short_path() {
        let result = extract_blog_domain("https://blog.hatena.ne.jp/onlyone");
        assert!(result.is_err());
    }
}
