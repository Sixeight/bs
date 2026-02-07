pub mod atom;
pub mod wsse;

use anyhow::{Context, Result};
use std::io::Read;

use crate::config::ResolvedBlogConfig;

pub struct HatenaClient {
    agent: ureq::Agent,
    username: String,
    password: String,
    blog_domain: String,
    owner: String,
}

impl HatenaClient {
    pub fn new(blog_domain: &str, config: &ResolvedBlogConfig) -> Self {
        let owner = config
            .owner
            .as_deref()
            .unwrap_or(&config.username)
            .to_string();
        Self {
            agent: ureq::Agent::new_with_defaults(),
            username: config.username.clone(),
            password: config.password.clone(),
            blog_domain: blog_domain.to_string(),
            owner,
        }
    }

    fn collection_url(&self) -> String {
        format!(
            "https://blog.hatena.ne.jp/{}/{}/atom/entry",
            self.owner, self.blog_domain
        )
    }

    fn page_collection_url(&self) -> String {
        format!(
            "https://blog.hatena.ne.jp/{}/{}/atom/page",
            self.owner, self.blog_domain
        )
    }

    fn wsse_header(&self) -> String {
        wsse::generate(&self.username, &self.password)
    }

    fn map_ureq_error(err: ureq::Error, action: &str) -> anyhow::Error {
        match err {
            ureq::Error::StatusCode(code) => {
                let msg = match code {
                    401 => "authentication failed (check username/password)",
                    403 => "forbidden (check permissions)",
                    404 => "not found",
                    _ => "server error",
                };
                anyhow::anyhow!("Failed to {}: HTTP {} — {}", action, code, msg)
            }
            other => anyhow::anyhow!("Failed to {}: {}", action, other),
        }
    }

    fn get_xml(&self, url: &str) -> Result<String> {
        let resp = self
            .agent
            .get(url)
            .header("X-WSSE", &self.wsse_header())
            .header("Accept", "application/xml")
            .call()
            .map_err(|e| Self::map_ureq_error(e, "fetch entries"))?;

        let mut body = String::new();
        resp.into_body()
            .as_reader()
            .read_to_string(&mut body)
            .context("Failed to read response body")?;
        Ok(body)
    }

    pub fn list_entries(&self, page: Option<&str>) -> Result<atom::Feed> {
        let default_url = self.collection_url();
        let url = page.unwrap_or(&default_url);
        let body = self.get_xml(url)?;
        atom::parse_feed(&body)
    }

    pub fn get_entry_by_url(&self, url: &str) -> Result<atom::Entry> {
        let body = self.get_xml(url)?;
        atom::parse_entry(&body)
    }

    fn post_xml(&self, url: &str, entry_xml: &str, action: &str) -> Result<atom::Entry> {
        let resp = self
            .agent
            .post(url)
            .header("X-WSSE", &self.wsse_header())
            .header("Content-Type", "application/xml")
            .send(entry_xml)
            .map_err(|e| Self::map_ureq_error(e, action))?;

        let mut body = String::new();
        resp.into_body()
            .as_reader()
            .read_to_string(&mut body)
            .context("Failed to read response body")?;
        atom::parse_entry(&body)
    }

    pub fn create_entry(&self, entry_xml: &str) -> Result<atom::Entry> {
        self.post_xml(&self.collection_url(), entry_xml, "create entry")
    }

    pub fn create_page(&self, entry_xml: &str) -> Result<atom::Entry> {
        self.post_xml(&self.page_collection_url(), entry_xml, "create page")
    }

    pub fn update_entry(&self, edit_url: &str, entry_xml: &str) -> Result<atom::Entry> {
        let resp = self
            .agent
            .put(edit_url)
            .header("X-WSSE", &self.wsse_header())
            .header("Content-Type", "application/xml")
            .send(entry_xml)
            .map_err(|e| Self::map_ureq_error(e, "update entry"))?;

        let mut body = String::new();
        resp.into_body()
            .as_reader()
            .read_to_string(&mut body)
            .context("Failed to read response body")?;
        atom::parse_entry(&body)
    }

    pub fn delete_entry(&self, edit_url: &str) -> Result<()> {
        self.agent
            .delete(edit_url)
            .header("X-WSSE", &self.wsse_header())
            .call()
            .map_err(|e| Self::map_ureq_error(e, "delete entry"))?;
        Ok(())
    }
}
