use anyhow::Result;
use std::io::Read;

use crate::client::atom;
use crate::config::Config;
use crate::entry::LocalEntry;
use crate::progress;

pub fn run(
    blog: &str,
    title: Option<&str>,
    draft: bool,
    custom_path: Option<&str>,
    page: bool,
    categories: Vec<String>,
) -> Result<()> {
    let config = Config::load(None)?;
    let blog_config = config.get_blog(blog)?;

    let mut body = String::new();
    std::io::stdin().read_to_string(&mut body)?;

    let entry = LocalEntry {
        title: title.unwrap_or("").to_string(),
        date: String::new(),
        edited: String::new(),
        url: None,
        edit_url: None,
        preview_url: None,
        draft,
        categories,
        custom_path: custom_path.map(|s| s.to_string()),
        body,
    };

    let is_tty = progress::stderr_is_tty();
    let stdout_tty = progress::stdout_is_tty();
    let mut spinner = progress::Spinner::new();

    if is_tty {
        progress::status(&mut spinner, "post  creating...");
    }

    let client = crate::client::HatenaClient::new(blog, blog_config);
    let atom_entry = entry.to_atom_entry();
    let xml = atom::build_entry_xml(&atom_entry);
    let created = if page {
        client.create_page(&xml)?
    } else {
        client.create_entry(&xml)?
    };

    let result = LocalEntry::from_atom(created);
    let dest = result.file_path(&blog_config.local_root, blog, blog_config.omit_domain);
    result.save(&dest)?;

    if is_tty {
        progress::finish(&format!("post  {}", dest.display()));
    } else {
        progress::log_store(&dest);
    }
    if !stdout_tty {
        println!("{}", dest.display());
    }

    Ok(())
}
