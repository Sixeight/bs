use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::config::Config;
use crate::entry::LocalEntry;

pub fn run(paths: &[PathBuf]) -> Result<()> {
    let config = Config::load(None)?;

    for path in paths {
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
        updated.save(&dest)?;
        eprintln!("{}", dest.display());
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
