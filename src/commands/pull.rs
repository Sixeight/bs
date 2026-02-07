use anyhow::Result;

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

    for blog_domain in &targets {
        let blog_config = config.get_blog(blog_domain)?;
        let client = HatenaClient::new(blog_domain, blog_config);

        let mut page = None;
        loop {
            let feed = client.list_entries(page.as_deref())?;

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
                entry.save(&path)?;
                eprintln!("{}", path.display());
            }

            match feed.next_url {
                Some(next) => page = Some(next),
                None => break,
            }
        }
    }

    Ok(())
}
