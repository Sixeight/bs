use anyhow::Result;

use crate::config::Config;

pub fn run() -> Result<()> {
    let config = Config::load(None)?;

    let mut blogs: Vec<_> = config.blogs.iter().collect();
    blogs.sort_by_key(|(domain, _)| domain.as_str());

    let max_width = blogs.iter().map(|(d, _)| d.len()).max().unwrap_or(0);

    for (domain, blog_config) in &blogs {
        println!(
            "{:<width$}   {}",
            domain,
            blog_config.local_root.display(),
            width = max_width
        );
    }

    Ok(())
}
