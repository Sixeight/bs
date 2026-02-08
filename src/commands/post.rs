use anyhow::Result;
use std::io::Read;

use crate::client::atom;
use crate::config::Config;
use crate::entry::LocalEntry;

pub fn run(
    blog: &str,
    title: Option<&str>,
    draft: bool,
    custom_path: Option<&str>,
    page: bool,
) -> Result<()> {
    let config = Config::load(None)?;
    let blog_config = config.get_blog(blog)?;

    let mut body = String::new();
    std::io::stdin().read_to_string(&mut body)?;

    let entry = LocalEntry {
        title: title.unwrap_or("").to_string(),
        date: String::new(),
        url: None,
        edit_url: None,
        preview_url: None,
        draft,
        categories: Vec::new(),
        custom_path: custom_path.map(|s| s.to_string()),
        body,
    };

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
    println!("{}", dest.display());
    eprintln!("       store {}", dest.display());

    Ok(())
}
