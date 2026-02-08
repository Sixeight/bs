pub mod atom;
pub mod wsse;

use anyhow::{Context, Result};
use tokio::task::JoinHandle;

use crate::config::ResolvedBlogConfig;
use crate::progress;

pub struct HatenaClient {
    client: reqwest::Client,
    rt: tokio::runtime::Runtime,
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
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build runtime");
        let client = {
            let _guard = rt.enter();
            reqwest::Client::builder()
                .http2_adaptive_window(true)
                .http2_initial_stream_window_size(2 * 1024 * 1024)
                .http2_initial_connection_window_size(4 * 1024 * 1024)
                .http2_keep_alive_interval(Some(std::time::Duration::from_secs(20)))
                .http2_keep_alive_while_idle(true)
                .tcp_nodelay(true)
                .pool_max_idle_per_host(1)
                .build()
                .expect("Failed to build HTTP client")
        };
        Self {
            client,
            rt,
            username: config.username.clone(),
            password: config.password.clone(),
            blog_domain: blog_domain.to_string(),
            owner,
        }
    }

    pub fn collection_url(&self) -> String {
        format!(
            "https://blog.hatena.ne.jp/{}/{}/atom/entry",
            self.owner, self.blog_domain
        )
    }

    pub fn page_collection_url(&self) -> String {
        format!(
            "https://blog.hatena.ne.jp/{}/{}/atom/page",
            self.owner, self.blog_domain
        )
    }

    fn wsse_header(&self) -> String {
        wsse::generate(&self.username, &self.password)
    }

    fn map_error(err: reqwest::Error, action: &str) -> anyhow::Error {
        if let Some(status) = err.status() {
            let msg = match status.as_u16() {
                401 => "authentication failed (check username/password)",
                403 => "forbidden (check permissions)",
                404 => "not found",
                _ => "server error",
            };
            anyhow::anyhow!("Failed to {}: HTTP {} — {}", action, status.as_u16(), msg)
        } else {
            anyhow::anyhow!("Failed to {}: {}", action, err)
        }
    }

    fn send_request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&str>,
        action: &str,
    ) -> Result<String> {
        progress::log_http_request(method.as_str(), url);
        self.rt.block_on(async {
            let mut builder = self
                .client
                .request(method, url)
                .header("X-WSSE", self.wsse_header());
            if let Some(body) = body {
                builder = builder
                    .header("Content-Type", "application/xml")
                    .body(body.to_string());
            } else {
                builder = builder.header("Accept", "application/xml");
            }
            let resp = builder
                .send()
                .await
                .map_err(|e| Self::map_error(e, action))?;
            progress::log_http_response(resp.status().as_u16(), url);
            resp.error_for_status()
                .map_err(|e| Self::map_error(e, action))?
                .text()
                .await
                .context("Failed to read response body")
        })
    }

    pub fn get_xml(&self, url: &str) -> Result<String> {
        self.send_request(reqwest::Method::GET, url, None, "fetch entries")
    }

    pub fn spawn_fetch(&self, url: String) -> JoinHandle<Result<String>> {
        let client = self.client.clone();
        let wsse = self.wsse_header();
        progress::log_http_request("GET", &url);
        self.rt.spawn(async move {
            let resp = client
                .get(&url)
                .header("X-WSSE", wsse)
                .header("Accept", "application/xml")
                .send()
                .await
                .map_err(|e| Self::map_error(e, "fetch entries"))?;
            progress::log_http_response(resp.status().as_u16(), &url);
            resp.error_for_status()
                .map_err(|e| Self::map_error(e, "fetch entries"))?
                .text()
                .await
                .context("Failed to read response body")
        })
    }

    pub fn await_fetch(&self, handle: JoinHandle<Result<String>>) -> Result<String> {
        self.rt
            .block_on(handle)
            .context("Fetch task panicked")?
    }

    pub fn get_entry_by_url(&self, url: &str) -> Result<atom::Entry> {
        atom::parse_entry(&self.get_xml(url)?)
    }

    pub fn create_entry(&self, entry_xml: &str) -> Result<atom::Entry> {
        let url = self.collection_url();
        atom::parse_entry(&self.send_request(reqwest::Method::POST, &url, Some(entry_xml), "create entry")?)
    }

    pub fn create_page(&self, entry_xml: &str) -> Result<atom::Entry> {
        let url = self.page_collection_url();
        atom::parse_entry(&self.send_request(reqwest::Method::POST, &url, Some(entry_xml), "create page")?)
    }

    pub fn update_entry(&self, edit_url: &str, entry_xml: &str) -> Result<atom::Entry> {
        atom::parse_entry(&self.send_request(reqwest::Method::PUT, edit_url, Some(entry_xml), "update entry")?)
    }

    pub fn delete_entry(&self, edit_url: &str) -> Result<()> {
        self.send_request(reqwest::Method::DELETE, edit_url, None, "delete entry")?;
        Ok(())
    }
}
