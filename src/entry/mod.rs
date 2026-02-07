use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::client::atom;

#[derive(Debug, Clone)]
pub struct LocalEntry {
    pub title: String,
    pub date: String,
    pub url: Option<String>,
    pub edit_url: Option<String>,
    pub preview_url: Option<String>,
    pub draft: bool,
    pub categories: Vec<String>,
    pub custom_path: Option<String>,
    pub body: String,
}

impl LocalEntry {
    pub fn from_atom(entry: atom::Entry) -> Self {
        Self {
            title: entry.title,
            date: entry.updated,
            url: entry.alternate_url,
            edit_url: entry.edit_url,
            preview_url: entry.preview_url,
            draft: entry.draft,
            categories: entry.categories,
            custom_path: entry.custom_path,
            body: entry.content,
        }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read entry file")?;
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self> {
        let content = content.trim_start();
        if !content.starts_with("---") {
            anyhow::bail!("Entry file must start with YAML frontmatter (---)");
        }

        let rest = &content[3..];
        let end = rest
            .find("\n---")
            .context("No closing --- for frontmatter")?;
        let frontmatter = &rest[..end];
        let body = rest[end + 4..].trim_start_matches('\n').to_string();

        let header: FrontMatter =
            serde_yml::from_str(frontmatter).context("Failed to parse frontmatter")?;

        Ok(Self {
            title: header.title.unwrap_or_default(),
            date: header.date.unwrap_or_default(),
            url: header.url,
            edit_url: header.edit_url,
            preview_url: header.preview_url,
            draft: header.draft.unwrap_or(false),
            categories: header.category.unwrap_or_default(),
            custom_path: header.custom_path,
            body,
        })
    }

    pub fn write_to_string(&self, out: &mut String) {
        let omit_date = self.draft && is_date_in_past(&self.date);
        let omit_url = self.draft && self.url.as_deref().map_or(false, |u| {
            is_likely_given_path(extract_url_path(u).unwrap_or(""))
        });

        out.reserve(256 + self.body.len());
        out.push_str("---\n");
        out.push_str("Title: ");
        if needs_yaml_quoting(&self.title) {
            out.push('\'');
            // Escape single quotes by doubling them
            for c in self.title.chars() {
                if c == '\'' {
                    out.push_str("''");
                } else {
                    out.push(c);
                }
            }
            out.push('\'');
        } else {
            out.push_str(&self.title);
        }
        out.push('\n');
        if !self.categories.is_empty() {
            out.push_str("Category:\n");
            for cat in &self.categories {
                out.push_str("- ");
                out.push_str(cat);
                out.push('\n');
            }
        }
        if !omit_date {
            out.push_str("Date: ");
            out.push_str(&self.date);
            out.push('\n');
        }
        if !omit_url {
            if let Some(ref url) = self.url {
                out.push_str("URL: ");
                out.push_str(url);
                out.push('\n');
            }
        }
        if let Some(ref edit_url) = self.edit_url {
            out.push_str("EditURL: ");
            out.push_str(edit_url);
            out.push('\n');
        }
        if let Some(ref preview_url) = self.preview_url {
            out.push_str("PreviewURL: ");
            out.push_str(preview_url);
            out.push('\n');
        }
        if self.draft {
            out.push_str("Draft: true\n");
        }
        if let Some(ref custom_path) = self.custom_path {
            out.push_str("CustomPath: ");
            out.push_str(custom_path);
            out.push('\n');
        }
        out.push_str("---\n\n");
        out.push_str(&self.body);
        if !self.body.is_empty() && !self.body.ends_with('\n') {
            out.push('\n');
        }
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        self.write_to_string(&mut out);
        out
    }

    pub fn to_atom_entry(&self) -> atom::Entry {
        atom::Entry {
            title: self.title.clone(),
            content: self.body.clone(),
            updated: self.date.clone(),
            published: self.date.clone(),
            edit_url: self.edit_url.clone(),
            alternate_url: self.url.clone(),
            preview_url: self.preview_url.clone(),
            draft: self.draft,
            categories: self.categories.clone(),
            custom_path: self.custom_path.clone(),
            author_name: None,
        }
    }

    fn base_dir(local_root: &Path, blog_domain: &str, omit_domain: bool) -> PathBuf {
        if omit_domain {
            local_root.to_path_buf()
        } else {
            local_root.join(blog_domain)
        }
    }

    pub fn file_path(&self, local_root: &Path, blog_domain: &str, omit_domain: bool) -> PathBuf {
        let base = Self::base_dir(local_root, blog_domain, omit_domain);

        // Draft with auto-generated path → _draft/{entryID}.md
        if self.draft {
            let url_path = self.url.as_deref().and_then(extract_url_path).unwrap_or("");
            if is_likely_given_path(url_path) {
                if let Some(entry_id) = self.edit_url.as_deref().and_then(extract_entry_id) {
                    return base.join("entry").join("_draft").join(format!("{entry_id}.md"));
                }
            }
        }

        if let Some(ref url) = self.url {
            Self::path_from_url(url, local_root, blog_domain, omit_domain)
        } else {
            match self.custom_path {
                Some(ref cp) => base.join("entry").join(cp.trim_start_matches('/')),
                None => base.join("entry").join("unknown.md"),
            }
        }
    }

    pub fn path_from_url(
        url: &str,
        local_root: &Path,
        blog_domain: &str,
        omit_domain: bool,
    ) -> PathBuf {
        let path = extract_url_path(url).unwrap_or("entry/unknown.md");

        let base = Self::base_dir(local_root, blog_domain, omit_domain);
        let file_path = base.join(path);
        if file_path.extension().is_none() {
            file_path.with_extension("md")
        } else {
            file_path
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create directory")?;
        }
        std::fs::write(path, self.to_string()).context("Failed to write entry file")?;
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FrontMatter {
    title: Option<String>,
    category: Option<Vec<String>>,
    date: Option<String>,
    #[serde(alias = "URL")]
    url: Option<String>,
    #[serde(alias = "EditURL")]
    edit_url: Option<String>,
    #[serde(alias = "PreviewURL")]
    preview_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_draft")]
    draft: Option<bool>,
    custom_path: Option<String>,
}

fn extract_url_path(url: &str) -> Option<&str> {
    let after_proto = &url[url.find("://")? + 3..];
    let path = &after_proto[after_proto.find('/')? + 1..];
    let path = path.split('?').next().unwrap_or(path);
    let path = path.split('#').next().unwrap_or(path);
    if path.is_empty() { None } else { Some(path) }
}

/// Check if the URL path looks like an auto-generated path by Hatena Blog.
/// Patterns: `entry/YYYY/MM/DD/HHMMSS` or `entry/YYYYMMDD/timestamp`
fn is_likely_given_path(path: &str) -> bool {
    let entry_path = path.strip_prefix("entry/").unwrap_or(path);
    // YYYY/MM/DD/HHMMSS
    if entry_path.len() >= 13 {
        let parts: Vec<&str> = entry_path.splitn(5, '/').collect();
        if parts.len() >= 4
            && parts[0].len() == 4 && parts[0].bytes().all(|b| b.is_ascii_digit())
            && parts[1].len() == 2 && parts[1].bytes().all(|b| b.is_ascii_digit())
            && parts[2].len() == 2 && parts[2].bytes().all(|b| b.is_ascii_digit())
            && parts[3].bytes().all(|b| b.is_ascii_digit())
        {
            return true;
        }
    }
    // YYYYMMDD/timestamp
    {
        let parts: Vec<&str> = entry_path.splitn(3, '/').collect();
        if parts.len() >= 2
            && parts[0].len() == 8 && parts[0].bytes().all(|b| b.is_ascii_digit())
            && parts[1].bytes().all(|b| b.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Extract entry ID from an EditURL like
/// `https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/123456`
fn extract_entry_id(edit_url: &str) -> Option<&str> {
    edit_url.rsplit('/').next().filter(|s| !s.is_empty())
}

/// Check if the date string represents a time in the past.
fn is_date_in_past(date: &str) -> bool {
    if date.is_empty() {
        return true;
    }
    let Ok(dt) = OffsetDateTime::parse(date, &Iso8601::DEFAULT) else {
        return true;
    };
    dt < OffsetDateTime::now_utc()
}

/// Check if a YAML scalar value needs single-quoting.
/// Matches Go's YAML marshaler behavior for special characters.
fn needs_yaml_quoting(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with(' ') || s.ends_with(' ') || s.starts_with('#') || s.ends_with(':') {
        return true;
    }
    if s.contains('{') || s.contains('}') || s.contains('[') || s.contains(']') {
        return true;
    }
    // # preceded by space triggers quoting
    if s.contains(" #") {
        return true;
    }
    // : followed by space triggers quoting
    if s.contains(": ") {
        return true;
    }
    false
}

fn deserialize_draft<'de, D>(deserializer: D) -> std::result::Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum DraftValue {
        Bool(bool),
        Str(String),
    }

    Option::<DraftValue>::deserialize(deserializer).map(|opt| {
        opt.map(|v| match v {
            DraftValue::Bool(b) => b,
            DraftValue::Str(s) => s.eq_ignore_ascii_case("yes"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1. parse() basic — standard frontmatter + body
    #[test]
    fn test_parse_basic() {
        let content = "\
---
Title: Hello World
Category:
- Rust
- Programming
Date: 2024-01-01T00:00:00+09:00
URL: https://example.com/entry/2024/01/01/hello
EditURL: https://blog.hatena.ne.jp/user/example.com/atom/entry/123
---
Body text here.
";
        let entry = LocalEntry::parse(content).unwrap();
        assert_eq!(entry.title, "Hello World");
        assert_eq!(entry.categories, vec!["Rust", "Programming"]);
        assert_eq!(entry.date, "2024-01-01T00:00:00+09:00");
        assert_eq!(
            entry.url,
            Some("https://example.com/entry/2024/01/01/hello".to_string())
        );
        assert_eq!(
            entry.edit_url,
            Some("https://blog.hatena.ne.jp/user/example.com/atom/entry/123".to_string())
        );
        assert!(!entry.draft);
        assert_eq!(entry.body, "Body text here.\n");
    }

    // 2. parse() Draft compatibility — Draft: yes / Draft: no
    #[test]
    fn test_parse_draft_yes_no() {
        let yes = "---\nTitle: T\nDraft: yes\n---\nbody";
        let entry = LocalEntry::parse(yes).unwrap();
        assert!(entry.draft);

        let no = "---\nTitle: T\nDraft: no\n---\nbody";
        let entry = LocalEntry::parse(no).unwrap();
        assert!(!entry.draft);
    }

    // 3. parse() Draft compatibility — Draft: true / Draft: false
    #[test]
    fn test_parse_draft_true_false() {
        let t = "---\nTitle: T\nDraft: true\n---\nbody";
        let entry = LocalEntry::parse(t).unwrap();
        assert!(entry.draft);

        let f = "---\nTitle: T\nDraft: false\n---\nbody";
        let entry = LocalEntry::parse(f).unwrap();
        assert!(!entry.draft);

        // Draft defaults to false when omitted
        let omitted = "---\nTitle: T\n---\nbody";
        let entry = LocalEntry::parse(omitted).unwrap();
        assert!(!entry.draft);
    }

    // 4. parse() PreviewURL — frontmatter with PreviewURL
    #[test]
    fn test_parse_preview_url() {
        let content = "\
---
Title: Draft Post
Date: 2024-01-01
PreviewURL: https://blog.example.com/preview/123
Draft: yes
---
Draft body";
        let entry = LocalEntry::parse(content).unwrap();
        assert_eq!(
            entry.preview_url,
            Some("https://blog.example.com/preview/123".to_string())
        );
        assert!(entry.draft);
    }

    // 5. parse() error — missing frontmatter
    #[test]
    fn test_parse_no_frontmatter() {
        let content = "No frontmatter here";
        let err = LocalEntry::parse(content).unwrap_err();
        assert!(
            err.to_string().contains("frontmatter"),
            "ERROR: expected frontmatter error, got: {err}"
        );
    }

    // 6. parse() error — missing closing separator
    #[test]
    fn test_parse_no_closing_separator() {
        let content = "---\nTitle: Test\nNo closing delimiter";
        let err = LocalEntry::parse(content).unwrap_err();
        assert!(
            err.to_string().contains("closing"),
            "ERROR: expected closing separator error, got: {err}"
        );
    }

    // 7. to_string() — parse -> to_string -> parse roundtrip (non-draft)
    #[test]
    fn test_to_string_roundtrip() {
        let entry = LocalEntry {
            title: "Roundtrip Test".to_string(),
            date: "2024-06-15T10:30:00+09:00".to_string(),
            url: Some("https://example.com/entry/2024/06/15/test".to_string()),
            edit_url: Some(
                "https://blog.hatena.ne.jp/user/example.com/atom/entry/456".to_string(),
            ),
            preview_url: Some("https://blog.example.com/preview/456".to_string()),
            draft: false,
            categories: vec!["Rust".to_string(), "CLI".to_string()],
            custom_path: Some("2024/06/15/test".to_string()),
            body: "Content body\n".to_string(),
        };

        let serialized = entry.to_string();
        let parsed = LocalEntry::parse(&serialized).unwrap();

        assert_eq!(parsed.title, entry.title);
        assert_eq!(parsed.date, entry.date);
        assert_eq!(parsed.url, entry.url);
        assert_eq!(parsed.edit_url, entry.edit_url);
        assert_eq!(parsed.preview_url, entry.preview_url);
        assert_eq!(parsed.draft, entry.draft);
        assert_eq!(parsed.categories, entry.categories);
        assert_eq!(parsed.custom_path, entry.custom_path);
        assert_eq!(parsed.body, entry.body);

        // Draft line should be omitted when draft=false
        assert!(!serialized.contains("Draft:"));

        // Draft: true should be serialized
        let draft_entry = LocalEntry {
            draft: true,
            ..entry
        };
        assert!(draft_entry.to_string().contains("Draft: true"));
    }

    // 8. file_path() — URL-based path generation
    #[test]
    fn test_file_path_from_url() {
        let entry = LocalEntry {
            title: "Test".to_string(),
            date: String::new(),
            url: Some("https://example.com/entry/2024/01/01/test".to_string()),
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: String::new(),
        };

        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/2024/01/01/test.md")
        );
    }

    // 9. file_path() — CustomPath-based
    #[test]
    fn test_file_path_custom_path() {
        let entry = LocalEntry {
            title: "Test".to_string(),
            date: String::new(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: Some("custom/path/name".to_string()),
            body: String::new(),
        };

        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/custom/path/name")
        );

        // Falls back to unknown.md when no URL or CustomPath
        let fallback = LocalEntry {
            custom_path: None,
            ..entry
        };
        let path = fallback.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/unknown.md")
        );
    }

    // 10. file_path() — omit_domain=true
    #[test]
    fn test_file_path_omit_domain() {
        let entry = LocalEntry {
            title: "Test".to_string(),
            date: String::new(),
            url: Some("https://example.com/entry/2024/01/01/test".to_string()),
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: String::new(),
        };

        // omit_domain=false: domain is included in path
        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/2024/01/01/test.md")
        );

        // omit_domain=true: domain is omitted from path
        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", true);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/entry/2024/01/01/test.md")
        );
    }

    // 11. Draft: YES/Yes — case insensitive
    #[test]
    fn test_parse_draft_case_insensitive() {
        for val in &["YES", "Yes", "yEs"] {
            let content = format!("---\nTitle: T\nDraft: {val}\n---\nbody");
            let entry = LocalEntry::parse(&content).unwrap();
            assert!(entry.draft, "ERROR: Draft: {val} should be true");
        }

        for val in &["NO", "No", "anything_else"] {
            let content = format!("---\nTitle: T\nDraft: {val}\n---\nbody");
            let entry = LocalEntry::parse(&content).unwrap();
            assert!(!entry.draft, "ERROR: Draft: {val} should be false");
        }
    }

    // 12. from_atom -> to_atom_entry roundtrip
    #[test]
    fn test_atom_roundtrip() {
        let atom_entry = crate::client::atom::Entry {
            title: "Atom Test".to_string(),
            content: "Atom body\n".to_string(),
            updated: "2024-03-01T12:00:00+09:00".to_string(),
            published: "2024-03-01T12:00:00+09:00".to_string(),
            edit_url: Some("https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/789".to_string()),
            alternate_url: Some("https://blog.example.com/entry/2024/03/01/test".to_string()),
            preview_url: None,
            draft: true,
            categories: vec!["Tech".to_string()],
            custom_path: None,
            author_name: Some("testuser".to_string()),
        };

        let local = LocalEntry::from_atom(atom_entry.clone());
        assert_eq!(local.title, "Atom Test");
        assert_eq!(local.body, "Atom body\n");
        assert!(local.draft);
        assert_eq!(local.categories, vec!["Tech"]);

        let back = local.to_atom_entry();
        assert_eq!(back.title, atom_entry.title);
        assert_eq!(back.content, atom_entry.content);
        assert_eq!(back.draft, atom_entry.draft);
        assert_eq!(back.categories, atom_entry.categories);
        assert_eq!(back.edit_url, atom_entry.edit_url);
        assert_eq!(back.alternate_url, atom_entry.alternate_url);
    }

    // 13. to_string() — minimal entry (no optional fields)
    #[test]
    fn test_to_string_minimal() {
        let entry = LocalEntry {
            title: "Minimal".to_string(),
            date: "2024-01-01".to_string(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: "content".to_string(),
        };
        let output = entry.to_string();
        assert!(output.starts_with("---\n"));
        assert!(output.contains("Title: Minimal\n"));
        assert!(output.contains("Date: 2024-01-01\n"));
        assert!(!output.contains("URL:"));
        assert!(!output.contains("EditURL:"));
        assert!(!output.contains("PreviewURL:"));
        assert!(!output.contains("Draft:"));
        assert!(!output.contains("Category:"));
        assert!(!output.contains("CustomPath:"));
        assert!(output.ends_with("---\n\ncontent\n"));
    }

    // 14. path_from_url — URL with extension is preserved
    #[test]
    fn test_path_from_url_with_extension() {
        let path = LocalEntry::path_from_url(
            "https://example.com/entry/2024/01/01/test.html",
            Path::new("/tmp/blog"),
            "example.com",
            false,
        );
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/2024/01/01/test.html")
        );
    }

    // 15. from_file + save file I/O roundtrip (non-draft preserves all fields)
    #[test]
    fn test_file_io_roundtrip() {
        let dir = std::env::temp_dir().join("bs_test_file_io");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let entry = LocalEntry {
            title: "File IO Test".to_string(),
            date: "2024-06-01".to_string(),
            url: Some("https://example.com/entry/test".to_string()),
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: vec!["Test".to_string()],
            custom_path: None,
            body: "File content\n".to_string(),
        };

        let path = dir.join("test_entry.md");
        entry.save(&path).unwrap();

        let loaded = LocalEntry::from_file(&path).unwrap();
        assert_eq!(loaded.title, entry.title);
        assert_eq!(loaded.date, entry.date);
        assert_eq!(loaded.url, entry.url);
        assert_eq!(loaded.draft, entry.draft);
        assert_eq!(loaded.categories, entry.categories);
        assert_eq!(loaded.body, entry.body);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // All fields combined parse test
    #[test]
    fn test_parse_full_frontmatter() {
        let content = "\
---
Title: Full Entry
Category:
- Rust
- CLI
Date: 2024-03-15T12:00:00+09:00
URL: https://example.com/entry/2024/03/15/full
EditURL: https://blog.hatena.ne.jp/user/example.com/atom/entry/789
PreviewURL: https://blog.example.com/preview/789
Draft: yes
CustomPath: 2024/03/15/full
---
Full body content here.
";
        let entry = LocalEntry::parse(content).unwrap();
        assert_eq!(entry.title, "Full Entry");
        assert_eq!(entry.categories, vec!["Rust", "CLI"]);
        assert_eq!(entry.date, "2024-03-15T12:00:00+09:00");
        assert_eq!(
            entry.url,
            Some("https://example.com/entry/2024/03/15/full".to_string())
        );
        assert_eq!(
            entry.edit_url,
            Some("https://blog.hatena.ne.jp/user/example.com/atom/entry/789".to_string())
        );
        assert_eq!(
            entry.preview_url,
            Some("https://blog.example.com/preview/789".to_string())
        );
        assert!(entry.draft);
        assert_eq!(entry.custom_path, Some("2024/03/15/full".to_string()));
        assert_eq!(entry.body, "Full body content here.\n");
    }

    // 16. parse() — empty body
    #[test]
    fn test_parse_empty_body() {
        let content = "---\nTitle: Empty\nDate: 2024-01-01\n---\n";
        let entry = LocalEntry::parse(content).unwrap();
        assert_eq!(entry.title, "Empty");
        assert_eq!(entry.body, "");
    }

    // 17. is_likely_given_path — auto-generated path patterns
    #[test]
    fn test_is_likely_given_path() {
        // YYYY/MM/DD/HHMMSS
        assert!(is_likely_given_path("entry/2024/01/15/120000"));
        assert!(is_likely_given_path("2024/01/15/120000"));
        // YYYYMMDD/timestamp
        assert!(is_likely_given_path("entry/20240115/1705286400"));
        assert!(is_likely_given_path("20240115/1705286400"));
        // Custom paths are NOT auto-generated
        assert!(!is_likely_given_path("entry/2024/01/15/my-post"));
        assert!(!is_likely_given_path("entry/custom/path"));
        assert!(!is_likely_given_path("entry/about"));
    }

    // 18. extract_entry_id
    #[test]
    fn test_extract_entry_id() {
        assert_eq!(
            extract_entry_id("https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/123456"),
            Some("123456")
        );
        assert_eq!(extract_entry_id("https://example.com/"), None);
    }

    // 19. file_path — draft with auto-generated path → _draft/{id}.md
    #[test]
    fn test_file_path_draft_auto_generated() {
        let entry = LocalEntry {
            title: "Draft".to_string(),
            date: String::new(),
            url: Some("https://example.com/entry/2024/01/15/120000".to_string()),
            edit_url: Some(
                "https://blog.hatena.ne.jp/user/example.com/atom/entry/456789".to_string(),
            ),
            preview_url: None,
            draft: true,
            categories: Vec::new(),
            custom_path: None,
            body: String::new(),
        };

        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/_draft/456789.md")
        );
    }

    // 20. file_path — draft with custom path → normal URL path (not _draft)
    #[test]
    fn test_file_path_draft_custom_path() {
        let entry = LocalEntry {
            title: "Draft Custom".to_string(),
            date: String::new(),
            url: Some("https://example.com/entry/my-post".to_string()),
            edit_url: Some(
                "https://blog.hatena.ne.jp/user/example.com/atom/entry/789".to_string(),
            ),
            preview_url: None,
            draft: true,
            categories: Vec::new(),
            custom_path: None,
            body: String::new(),
        };

        let path = entry.file_path(Path::new("/tmp/blog"), "example.com", false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/blog/example.com/entry/my-post.md")
        );
    }

    // 21. write_to_string — draft omits Date when past, omits URL when auto-generated
    #[test]
    fn test_write_draft_omits_date_and_url() {
        let entry = LocalEntry {
            title: "Draft".to_string(),
            date: "2024-01-01T00:00:00+09:00".to_string(),
            url: Some("https://example.com/entry/2024/01/01/120000".to_string()),
            edit_url: Some("https://blog.hatena.ne.jp/user/example.com/atom/entry/123".to_string()),
            preview_url: None,
            draft: true,
            categories: Vec::new(),
            custom_path: None,
            body: "body\n".to_string(),
        };

        let output = entry.to_string();
        assert!(!output.contains("\nDate:"), "Date should be omitted for draft with past date");
        assert!(!output.contains("\nURL:"), "URL should be omitted for draft with auto-generated path");
        assert!(output.contains("Draft: true"));
    }

    // 22. write_to_string — draft with custom URL preserves URL
    #[test]
    fn test_write_draft_preserves_custom_url() {
        let entry = LocalEntry {
            title: "Draft".to_string(),
            date: "2024-01-01T00:00:00+09:00".to_string(),
            url: Some("https://example.com/entry/my-post".to_string()),
            edit_url: None,
            preview_url: None,
            draft: true,
            categories: Vec::new(),
            custom_path: None,
            body: "body\n".to_string(),
        };

        let output = entry.to_string();
        assert!(!output.contains("Date:"), "Date should be omitted for draft with past date");
        assert!(output.contains("URL: https://example.com/entry/my-post"), "Custom URL should be preserved");
    }

    // 23. write_to_string — trailing newline guaranteed
    #[test]
    fn test_write_trailing_newline() {
        let entry = LocalEntry {
            title: "Test".to_string(),
            date: "2024-01-01".to_string(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: "no trailing newline".to_string(),
        };

        let output = entry.to_string();
        assert!(output.ends_with("no trailing newline\n"));
    }

    // 24. write_to_string — empty body does not add extra newline
    #[test]
    fn test_write_empty_body_no_extra_newline() {
        let entry = LocalEntry {
            title: "Test".to_string(),
            date: "2024-01-01".to_string(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: String::new(),
        };

        let output = entry.to_string();
        assert!(output.ends_with("---\n\n"));
    }

    // 25. needs_yaml_quoting — matches Go YAML marshaler behavior
    #[test]
    fn test_needs_yaml_quoting() {
        // Leading/trailing space
        assert!(needs_yaml_quoting(" leading"));
        assert!(needs_yaml_quoting("trailing "));
        // # preceded by space or at start
        assert!(needs_yaml_quoting("#hashtag"));
        assert!(needs_yaml_quoting("hello #world"));
        // # not preceded by space → no quoting
        assert!(!needs_yaml_quoting("Module#method"));
        assert!(!needs_yaml_quoting("LiveCoding#8"));
        // : followed by space or at end
        assert!(needs_yaml_quoting("Re: something"));
        assert!(needs_yaml_quoting("key:"));
        // : not followed by space → no quoting
        assert!(!needs_yaml_quoting("YAPC::Asia"));
        assert!(!needs_yaml_quoting("svn:external"));
        // Braces
        assert!(needs_yaml_quoting("{foo}"));
        assert!(needs_yaml_quoting("[bar]"));
        // Normal strings
        assert!(!needs_yaml_quoting("Hello World"));
        assert!(!needs_yaml_quoting(""));
    }

    // 26. title quoting roundtrip
    #[test]
    fn test_title_quoting_roundtrip() {
        let entry = LocalEntry {
            title: " leading space".to_string(),
            date: "2024-01-01".to_string(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: "body\n".to_string(),
        };
        let output = entry.to_string();
        assert!(output.contains("Title: ' leading space'"));
        let parsed = LocalEntry::parse(&output).unwrap();
        assert_eq!(parsed.title, " leading space");
    }

    // 27. title with single quotes is escaped
    #[test]
    fn test_title_single_quote_escaping() {
        let entry = LocalEntry {
            title: "it's a Re: test".to_string(),
            date: "2024-01-01".to_string(),
            url: None,
            edit_url: None,
            preview_url: None,
            draft: false,
            categories: Vec::new(),
            custom_path: None,
            body: "body\n".to_string(),
        };
        let output = entry.to_string();
        // : followed by space triggers quoting, ' is doubled
        assert!(output.contains("Title: 'it''s a Re: test'"));
        let parsed = LocalEntry::parse(&output).unwrap();
        assert_eq!(parsed.title, "it's a Re: test");
    }
}
