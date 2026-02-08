use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

#[derive(Debug, Clone)]
pub struct Feed {
    pub entries: Vec<Entry>,
    pub next_url: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Entry {
    pub title: String,
    pub content: String,
    pub updated: String,
    pub published: String,
    pub edited: String,
    pub edit_url: Option<String>,
    pub alternate_url: Option<String>,
    pub preview_url: Option<String>,
    pub draft: bool,
    pub categories: Vec<String>,
    pub custom_path: Option<String>,
    pub author_name: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tag {
    None,
    Title,
    Content,
    Updated,
    Published,
    AppEdited,
    AppDraft,
    FormattedContent,
    AuthorName,
    Other,
}

struct LinkAttrs {
    rel: LinkRel,
    href: String,
}

#[derive(PartialEq)]
enum LinkRel {
    Edit,
    Alternate,
    Preview,
    Next,
    Other,
}

fn extract_link_attrs(e: &BytesStart<'_>) -> LinkAttrs {
    let mut rel = LinkRel::Other;
    let mut href = String::new();
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"rel" => {
                rel = match attr.value.as_ref() {
                    b"edit" => LinkRel::Edit,
                    b"alternate" => LinkRel::Alternate,
                    b"preview" => LinkRel::Preview,
                    b"next" => LinkRel::Next,
                    _ => LinkRel::Other,
                };
            }
            b"href" => {
                href = String::from_utf8_lossy(&attr.value).into_owned();
            }
            _ => {}
        }
    }
    LinkAttrs { rel, href }
}

fn apply_link(attrs: LinkAttrs, entry: Option<&mut Entry>, next_url: &mut Option<String>) {
    match attrs.rel {
        LinkRel::Edit => {
            if let Some(e) = entry {
                e.edit_url = Some(attrs.href);
            }
        }
        LinkRel::Alternate => {
            if let Some(e) = entry {
                e.alternate_url = Some(attrs.href);
            }
        }
        LinkRel::Preview => {
            if let Some(e) = entry {
                e.preview_url = Some(attrs.href);
            }
        }
        LinkRel::Next => {
            if entry.is_none() {
                *next_url = Some(attrs.href);
            }
        }
        LinkRel::Other => {}
    }
}

pub fn parse_feed(xml: &str) -> Result<Feed> {
    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    let mut next_url = None;
    let mut current_entry: Option<Entry> = None;
    let mut current_tag = Tag::None;
    let mut in_author = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"entry" => {
                    current_entry = Some(Entry::default());
                }
                b"link" => {
                    let attrs = extract_link_attrs(&e);
                    apply_link(attrs, current_entry.as_mut(), &mut next_url);
                }
                b"author" => {
                    in_author = true;
                }
                b"title" => current_tag = Tag::Title,
                b"content" => current_tag = Tag::Content,
                b"updated" => current_tag = Tag::Updated,
                b"published" => current_tag = Tag::Published,
                name if name == b"app:edited" => current_tag = Tag::AppEdited,
                name if name == b"app:draft" => current_tag = Tag::AppDraft,
                name if name == b"hatena:formatted-content" => current_tag = Tag::FormattedContent,
                b"name" if in_author => current_tag = Tag::AuthorName,
                _ => current_tag = Tag::Other,
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"link" => {
                    let attrs = extract_link_attrs(&e);
                    apply_link(attrs, current_entry.as_mut(), &mut next_url);
                }
                b"category" => {
                    if let Some(ref mut entry) = current_entry {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"term" {
                                entry
                                    .categories
                                    .push(String::from_utf8_lossy(&attr.value).into_owned());
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if let Some(ref mut entry) = current_entry {
                    match current_tag {
                        Tag::Title => entry.title = e.unescape().unwrap_or_default().into_owned(),
                        Tag::Content => {
                            entry.content = e.unescape().unwrap_or_default().into_owned()
                        }
                        Tag::FormattedContent => {}
                        Tag::Updated => {
                            entry.updated =
                                String::from_utf8(e.into_inner().to_vec()).unwrap_or_default();
                        }
                        Tag::Published => {
                            entry.published =
                                String::from_utf8(e.into_inner().to_vec()).unwrap_or_default();
                        }
                        Tag::AppEdited => {
                            entry.edited =
                                String::from_utf8(e.into_inner().to_vec()).unwrap_or_default();
                        }
                        Tag::AppDraft => {
                            entry.draft = e.as_ref() == b"yes";
                        }
                        Tag::AuthorName => {
                            entry.author_name = Some(e.unescape().unwrap_or_default().into_owned())
                        }
                        Tag::None | Tag::Other => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    b"entry" => {
                        if let Some(entry) = current_entry.take() {
                            entries.push(entry);
                        }
                    }
                    b"author" => {
                        in_author = false;
                    }
                    _ => {}
                }
                current_tag = Tag::None;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow::anyhow!("XML parse error: {}", e))
                    .context("Failed to parse Atom feed");
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(Feed { entries, next_url })
}

pub fn parse_entry(xml: &str) -> Result<Entry> {
    let feed = parse_feed(xml)?;
    feed.entries
        .into_iter()
        .next()
        .context("No entry found in response")
}

pub fn build_entry_xml(entry: &Entry) -> String {
    let mut xml = String::with_capacity(512 + entry.content.len());
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    xml.push_str("<entry xmlns=\"http://www.w3.org/2005/Atom\"\n");
    xml.push_str("       xmlns:app=\"http://www.w3.org/2007/app\"\n");
    xml.push_str("       xmlns:hatena=\"http://www.hatena.ne.jp/info/xmlns#\">\n");
    xml.push_str("  <title>");
    escape_xml_into(&mut xml, &entry.title);
    xml.push_str("</title>\n");
    if !entry.updated.is_empty() {
        xml.push_str("  <updated>");
        escape_xml_into(&mut xml, &entry.updated);
        xml.push_str("</updated>\n");
    }
    xml.push_str("  <content>");
    escape_xml_into(&mut xml, &entry.content);
    xml.push_str("</content>\n");

    for cat in &entry.categories {
        xml.push_str("  <category term=\"");
        escape_xml_into(&mut xml, cat);
        xml.push_str("\" />\n");
    }

    if let Some(ref path) = entry.custom_path {
        xml.push_str("  <hatena:custom-path>");
        escape_xml_into(&mut xml, path);
        xml.push_str("</hatena:custom-path>\n");
    }

    if entry.draft {
        xml.push_str("  <app:control>\n");
        xml.push_str("    <app:draft>yes</app:draft>\n");
        xml.push_str("    <app:preview>yes</app:preview>\n");
        xml.push_str("  </app:control>\n");
    }
    xml.push_str("</entry>");
    xml
}

fn escape_xml_into(buf: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            '\'' => buf.push_str("&apos;"),
            _ => buf.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:app="http://www.w3.org/2007/app"
      xmlns:hatena="http://www.hatena.ne.jp/info/xmlns#">
  <link rel="next" href="https://blog.hatena.ne.jp/user/blog.example.com/atom/entry?page=2" />
  <entry>
    <title>Test Entry</title>
    <link rel="edit" href="https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/123" />
    <link rel="alternate" href="https://blog.example.com/entry/2024/01/01/test" />
    <content type="text/html">Hello World</content>
    <updated>2024-01-01T00:00:00+09:00</updated>
    <published>2024-01-01T00:00:00+09:00</published>
    <app:edited>2024-02-15T10:30:00+09:00</app:edited>
    <category term="Rust" />
    <category term="Programming" />
    <app:control>
      <app:draft>no</app:draft>
    </app:control>
    <author>
      <name>testuser</name>
    </author>
  </entry>
</feed>"#;

    const SAMPLE_DRAFT_ENTRY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:app="http://www.w3.org/2007/app">
  <entry>
    <title>Draft Entry</title>
    <content>Draft content</content>
    <updated>2024-02-01T00:00:00+09:00</updated>
    <published>2024-02-01T00:00:00+09:00</published>
    <app:control>
      <app:draft>yes</app:draft>
    </app:control>
  </entry>
</feed>"#;

    #[test]
    fn test_parse_feed_basic() {
        let feed = parse_feed(SAMPLE_FEED).unwrap();
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(
            feed.next_url.as_deref(),
            Some("https://blog.hatena.ne.jp/user/blog.example.com/atom/entry?page=2")
        );
    }

    #[test]
    fn test_parse_entry_fields() {
        let feed = parse_feed(SAMPLE_FEED).unwrap();
        let entry = &feed.entries[0];
        assert_eq!(entry.title, "Test Entry");
        assert_eq!(entry.content, "Hello World");
        assert_eq!(entry.updated, "2024-01-01T00:00:00+09:00");
        assert_eq!(entry.edited, "2024-02-15T10:30:00+09:00");
        assert_eq!(
            entry.edit_url.as_deref(),
            Some("https://blog.hatena.ne.jp/user/blog.example.com/atom/entry/123")
        );
        assert_eq!(
            entry.alternate_url.as_deref(),
            Some("https://blog.example.com/entry/2024/01/01/test")
        );
        assert!(!entry.draft);
        assert_eq!(entry.categories, vec!["Rust", "Programming"]);
        assert_eq!(entry.author_name.as_deref(), Some("testuser"));
    }

    #[test]
    fn test_parse_draft_entry() {
        let entry = parse_entry(SAMPLE_DRAFT_ENTRY).unwrap();
        assert!(entry.draft);
        assert_eq!(entry.title, "Draft Entry");
    }

    #[test]
    fn test_parse_entry_from_single() {
        let entry = parse_entry(SAMPLE_FEED).unwrap();
        assert_eq!(entry.title, "Test Entry");
    }

    #[test]
    fn test_build_entry_xml_basic() {
        let entry = Entry {
            title: "My Title".to_string(),
            content: "Hello".to_string(),
            draft: false,
            categories: vec!["Rust".to_string()],
            ..Default::default()
        };
        let xml = build_entry_xml(&entry);
        assert!(xml.contains("<title>My Title</title>"));
        assert!(xml.contains("<content>Hello</content>"));
        assert!(!xml.contains("type=\"text/plain\""));
        assert!(
            !xml.contains("<app:control>"),
            "non-draft should not have app:control"
        );
        assert!(xml.contains("<category term=\"Rust\" />"));
    }

    #[test]
    fn test_build_entry_xml_draft() {
        let entry = Entry {
            title: "Draft".to_string(),
            draft: true,
            ..Default::default()
        };
        let xml = build_entry_xml(&entry);
        assert!(xml.contains("<app:draft>yes</app:draft>"));
        assert!(xml.contains("<app:preview>yes</app:preview>"));
        assert!(xml.contains("<app:control>"));
    }

    #[test]
    fn test_build_entry_xml_updated() {
        let entry = Entry {
            title: "Test".to_string(),
            updated: "2024-01-01T00:00:00+09:00".to_string(),
            ..Default::default()
        };
        let xml = build_entry_xml(&entry);
        assert!(xml.contains("<updated>2024-01-01T00:00:00+09:00</updated>"));

        // empty updated → no <updated> element
        let entry2 = Entry {
            title: "Test".to_string(),
            ..Default::default()
        };
        let xml2 = build_entry_xml(&entry2);
        assert!(!xml2.contains("<updated>"));
    }

    #[test]
    fn test_build_entry_xml_escapes_special_chars() {
        let entry = Entry {
            title: "A & B < C".to_string(),
            content: "foo \"bar\" 'baz'".to_string(),
            ..Default::default()
        };
        let xml = build_entry_xml(&entry);
        assert!(xml.contains("A &amp; B &lt; C"));
        assert!(xml.contains("foo &quot;bar&quot; &apos;baz&apos;"));
        // content should not have type attribute
        assert!(!xml.contains("type=\"text/plain\""));
    }

    #[test]
    fn test_build_entry_xml_custom_path() {
        let entry = Entry {
            title: "Test".to_string(),
            custom_path: Some("custom/path".to_string()),
            ..Default::default()
        };
        let xml = build_entry_xml(&entry);
        assert!(xml.contains("<hatena:custom-path>custom/path</hatena:custom-path>"));
    }

    #[test]
    fn test_parse_empty_feed() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        let feed = parse_feed(xml).unwrap();
        assert!(feed.entries.is_empty());
        assert!(feed.next_url.is_none());
    }

    #[test]
    fn test_parse_entry_no_entry_returns_error() {
        let xml = r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        assert!(parse_entry(xml).is_err());
    }

    #[test]
    fn test_formatted_content_ignored() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:hatena="http://www.hatena.ne.jp/info/xmlns#">
  <entry>
    <title>Test</title>
    <content type="text/plain">Original Markdown</content>
    <hatena:formatted-content type="text/html">&lt;p&gt;Rendered HTML&lt;/p&gt;</hatena:formatted-content>
    <updated>2024-01-01T00:00:00+09:00</updated>
    <published>2024-01-01T00:00:00+09:00</published>
  </entry>
</feed>"#;
        let entry = parse_entry(xml).unwrap();
        assert_eq!(entry.content, "Original Markdown");
    }
}
