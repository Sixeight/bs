use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct BlogConfig {
    pub username: Option<String>,
    pub password: Option<String>,
    pub local_root: Option<PathBuf>,
    #[serde(default)]
    pub omit_domain: Option<bool>,
    pub owner: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawConfig {
    pub default: Option<BlogConfig>,
    #[serde(flatten)]
    pub blogs: HashMap<String, BlogConfig>,
}

impl RawConfig {
    fn merge(local: RawConfig, global: RawConfig) -> RawConfig {
        let default = match (local.default, global.default) {
            (Some(l), Some(g)) => Some(BlogConfig::merge(l, g)),
            (Some(l), None) => Some(l),
            (None, Some(g)) => Some(g),
            (None, None) => None,
        };

        let mut blogs = global.blogs;
        for (domain, local_blog) in local.blogs {
            let merged = match blogs.remove(&domain) {
                Some(global_blog) => BlogConfig::merge(local_blog, global_blog),
                None => local_blog,
            };
            blogs.insert(domain, merged);
        }

        RawConfig { default, blogs }
    }
}

impl BlogConfig {
    fn merge(local: BlogConfig, global: BlogConfig) -> BlogConfig {
        BlogConfig {
            username: local.username.or(global.username),
            password: local.password.or(global.password),
            local_root: local.local_root.or(global.local_root),
            omit_domain: local.omit_domain.or(global.omit_domain),
            owner: local.owner.or(global.owner),
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub blogs: HashMap<String, ResolvedBlogConfig>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBlogConfig {
    pub username: String,
    pub password: String,
    pub local_root: PathBuf,
    pub omit_domain: bool,
    pub owner: Option<String>,
}

impl Config {
    pub fn load(workdir: Option<&Path>) -> Result<Self> {
        let local_path = Self::local_config_path(workdir);
        let global_path = Self::global_config_path();

        let local = local_path.and_then(|p| Self::read_config(&p));
        let global = global_path.and_then(|p| Self::read_config(&p));

        let raw = match (local, global) {
            (Some(l), Some(g)) => RawConfig::merge(l, g),
            (Some(l), None) => l,
            (None, Some(g)) => g,
            (None, None) => {
                anyhow::bail!(
                    "Config file not found. Create blogsync.yaml or ~/.config/blogsync/config.yaml"
                )
            }
        };

        Self::resolve(raw)
    }

    fn local_config_path(workdir: Option<&Path>) -> Option<PathBuf> {
        let path = match workdir {
            Some(w) => w.join("blogsync.yaml"),
            None => PathBuf::from("blogsync.yaml"),
        };
        if path.exists() { Some(path) } else { None }
    }

    fn global_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("blogsync").join("config.yaml"))
    }

    fn read_config(path: &Path) -> Option<RawConfig> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_yml::from_str(&content).ok()
    }

    fn resolve(raw: RawConfig) -> Result<Self> {
        let default = raw.default.unwrap_or(BlogConfig {
            username: None,
            password: None,
            local_root: None,
            omit_domain: None,
            owner: None,
        });

        let env_username = std::env::var("BLOGSYNC_USERNAME").ok();
        let env_password = std::env::var("BLOGSYNC_PASSWORD").ok();
        let env_workdir = std::env::var("BLOGSYNC_WORKDIR").ok().map(PathBuf::from);

        let mut blogs = HashMap::new();
        for (domain, blog) in raw.blogs {
            let username = blog
                .username
                .or_else(|| default.username.clone())
                .or_else(|| env_username.clone())
                .context(format!("No username configured for {}", domain))?;

            let password = blog
                .password
                .or_else(|| default.password.clone())
                .or_else(|| env_password.clone())
                .context(format!("No password configured for {}", domain))?;

            let local_root = blog
                .local_root
                .or_else(|| default.local_root.clone())
                .or_else(|| env_workdir.clone())
                .context(format!("No local_root configured for {}", domain))?;
            let local_root = expand_tilde(&local_root);

            let omit_domain = blog.omit_domain.or(default.omit_domain).unwrap_or(false);

            let owner = blog.owner.or_else(|| default.owner.clone());

            blogs.insert(
                domain,
                ResolvedBlogConfig {
                    username,
                    password,
                    local_root,
                    omit_domain,
                    owner,
                },
            );
        }

        Ok(Config { blogs })
    }

    pub fn get_blog(&self, domain: &str) -> Result<&ResolvedBlogConfig> {
        self.blogs
            .get(domain)
            .context(format!("Blog '{}' not found in config", domain))
    }

    /// Detect which blog a file belongs to by finding the longest matching path prefix.
    /// Returns (domain, config) for the best match.
    pub fn detect_blog_from_path(&self, path: &Path) -> Result<(&str, &ResolvedBlogConfig)> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .context("Failed to get current directory")?
                .join(path)
        };

        let mut best: Option<(&str, &ResolvedBlogConfig, usize)> = None;

        for (domain, config) in &self.blogs {
            // Try with domain in path
            let with_domain = config.local_root.join(domain);
            if abs_path.starts_with(&with_domain) {
                let len = with_domain.as_os_str().len();
                if best.as_ref().is_none_or(|(_, _, l)| len > *l) {
                    best = Some((domain.as_str(), config, len));
                }
            }
            // Try without domain (omit_domain case)
            if config.omit_domain && abs_path.starts_with(&config.local_root) {
                let len = config.local_root.as_os_str().len();
                if best.as_ref().is_none_or(|(_, _, l)| len > *l) {
                    best = Some((domain.as_str(), config, len));
                }
            }
        }

        best.map(|(d, c, _)| (d, c))
            .context(format!("No blog config matches path: {}", path.display()))
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_basic_config() {
        let raw = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "example.hatenablog.com".to_string(),
                BlogConfig {
                    username: Some("testuser".to_string()),
                    password: Some("testpass".to_string()),
                    local_root: Some(PathBuf::from("/tmp/blog")),
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("example.hatenablog.com").unwrap();
        assert_eq!(blog.username, "testuser");
        assert_eq!(blog.password, "testpass");
        assert_eq!(blog.local_root, PathBuf::from("/tmp/blog"));
        assert!(!blog.omit_domain);
    }

    #[test]
    fn test_resolve_with_default() {
        let raw = RawConfig {
            default: Some(BlogConfig {
                username: Some("defaultuser".to_string()),
                password: Some("defaultpass".to_string()),
                local_root: Some(PathBuf::from("/tmp/default")),
                omit_domain: Some(true),
                owner: None,
            }),
            blogs: HashMap::from([(
                "myblog.hateblo.jp".to_string(),
                BlogConfig {
                    username: None,
                    password: None,
                    local_root: None,
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("myblog.hateblo.jp").unwrap();
        assert_eq!(blog.username, "defaultuser");
        assert_eq!(blog.password, "defaultpass");
        assert_eq!(blog.local_root, PathBuf::from("/tmp/default"));
        assert!(blog.omit_domain);
    }

    #[test]
    fn test_resolve_missing_username() {
        let raw = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "example.hatenablog.com".to_string(),
                BlogConfig {
                    username: None,
                    password: Some("pass".to_string()),
                    local_root: Some(PathBuf::from("/tmp")),
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let result = Config::resolve(raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_tilde() {
        let path = PathBuf::from("~/Documents/blog");
        let expanded = expand_tilde(&path);
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("Documents/blog"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = PathBuf::from("/absolute/path");
        let expanded = expand_tilde(&path);
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_get_blog_not_found() {
        let config = Config {
            blogs: HashMap::new(),
        };
        assert!(config.get_blog("nonexistent.hatenablog.com").is_err());
    }

    #[test]
    fn test_parse_yaml_config() {
        let yaml = r#"
example.hatenablog.com:
  username: testuser
  password: testpass
  local_root: /tmp/blog
"#;
        let raw: RawConfig = serde_yml::from_str(yaml).unwrap();
        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("example.hatenablog.com").unwrap();
        assert_eq!(blog.username, "testuser");
    }

    #[test]
    fn test_parse_yaml_with_default_section() {
        let yaml = r#"
default:
  local_root: /tmp/blogs
blog1.hatenablog.com:
  username: user1
  password: pass1
blog2.hateblo.jp:
  username: user2
  password: pass2
"#;
        let raw: RawConfig = serde_yml::from_str(yaml).unwrap();
        let config = Config::resolve(raw).unwrap();
        assert_eq!(config.blogs.len(), 2);
        assert_eq!(
            config.get_blog("blog1.hatenablog.com").unwrap().local_root,
            PathBuf::from("/tmp/blogs")
        );
    }

    #[test]
    fn test_resolve_with_owner() {
        let raw = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "team.hatenablog.com".to_string(),
                BlogConfig {
                    username: Some("member".to_string()),
                    password: Some("pass".to_string()),
                    local_root: Some(PathBuf::from("/tmp/blog")),
                    omit_domain: None,
                    owner: Some("teamowner".to_string()),
                },
            )]),
        };

        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("team.hatenablog.com").unwrap();
        assert_eq!(blog.username, "member");
        assert_eq!(blog.owner, Some("teamowner".to_string()));
    }

    #[test]
    fn test_resolve_owner_from_default() {
        let raw = RawConfig {
            default: Some(BlogConfig {
                username: Some("defaultuser".to_string()),
                password: Some("defaultpass".to_string()),
                local_root: Some(PathBuf::from("/tmp")),
                omit_domain: None,
                owner: Some("defaultowner".to_string()),
            }),
            blogs: HashMap::from([(
                "blog.hateblo.jp".to_string(),
                BlogConfig {
                    username: None,
                    password: None,
                    local_root: None,
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("blog.hateblo.jp").unwrap();
        assert_eq!(blog.owner, Some("defaultowner".to_string()));
    }

    #[test]
    fn test_resolve_omit_domain_override() {
        let raw = RawConfig {
            default: Some(BlogConfig {
                username: Some("user".to_string()),
                password: Some("pass".to_string()),
                local_root: Some(PathBuf::from("/tmp")),
                omit_domain: Some(true),
                owner: None,
            }),
            blogs: HashMap::from([(
                "blog.example.com".to_string(),
                BlogConfig {
                    username: None,
                    password: None,
                    local_root: None,
                    omit_domain: Some(false),
                    owner: None,
                },
            )]),
        };

        let config = Config::resolve(raw).unwrap();
        let blog = config.get_blog("blog.example.com").unwrap();
        // Blog-specific config overrides default
        assert!(!blog.omit_domain);
    }

    #[test]
    fn test_merge_raw_configs_local_priority() {
        let local = RawConfig {
            default: Some(BlogConfig {
                username: Some("local_user".to_string()),
                password: None,
                local_root: Some(PathBuf::from("/local/root")),
                omit_domain: None,
                owner: None,
            }),
            blogs: HashMap::from([(
                "blog.example.com".to_string(),
                BlogConfig {
                    username: Some("blog_user".to_string()),
                    password: Some("blog_pass".to_string()),
                    local_root: None,
                    omit_domain: Some(true),
                    owner: None,
                },
            )]),
        };
        let global = RawConfig {
            default: Some(BlogConfig {
                username: Some("global_user".to_string()),
                password: Some("global_pass".to_string()),
                local_root: Some(PathBuf::from("/global/root")),
                omit_domain: Some(false),
                owner: Some("global_owner".to_string()),
            }),
            blogs: HashMap::from([(
                "blog.example.com".to_string(),
                BlogConfig {
                    username: Some("global_blog_user".to_string()),
                    password: Some("global_blog_pass".to_string()),
                    local_root: Some(PathBuf::from("/global/blog")),
                    omit_domain: Some(false),
                    owner: Some("global_blog_owner".to_string()),
                },
            )]),
        };

        let merged = RawConfig::merge(local, global);

        // default: local values win, global fills gaps
        let default = merged.default.unwrap();
        assert_eq!(default.username.as_deref(), Some("local_user"));
        assert_eq!(default.password.as_deref(), Some("global_pass"));
        assert_eq!(default.local_root, Some(PathBuf::from("/local/root")));
        assert_eq!(default.omit_domain, Some(false)); // global fills
        assert_eq!(default.owner.as_deref(), Some("global_owner"));

        // blog: local values win, global fills gaps
        let blog = merged.blogs.get("blog.example.com").unwrap();
        assert_eq!(blog.username.as_deref(), Some("blog_user"));
        assert_eq!(blog.password.as_deref(), Some("blog_pass"));
        assert_eq!(blog.local_root, Some(PathBuf::from("/global/blog"))); // global fills
        assert!(blog.omit_domain.unwrap()); // local wins
        assert_eq!(blog.owner.as_deref(), Some("global_blog_owner"));
    }

    #[test]
    fn test_merge_raw_configs_global_only_blog() {
        let local = RawConfig {
            default: None,
            blogs: HashMap::new(),
        };
        let global = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "global-only.hateblo.jp".to_string(),
                BlogConfig {
                    username: Some("user".to_string()),
                    password: Some("pass".to_string()),
                    local_root: Some(PathBuf::from("/tmp")),
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let merged = RawConfig::merge(local, global);
        assert!(merged.blogs.contains_key("global-only.hateblo.jp"));
    }

    #[test]
    fn test_global_config_path() {
        let path = Config::global_config_path();
        if let Some(p) = path {
            assert!(p.ends_with(".config/blogsync/config.yaml"));
            // Must NOT be under Library/Application Support on macOS
            assert!(!p.to_string_lossy().contains("Library/Application Support"));
        }
    }

    #[test]
    fn test_resolve_missing_password() {
        let raw = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "example.hatenablog.com".to_string(),
                BlogConfig {
                    username: Some("user".to_string()),
                    password: None,
                    local_root: Some(PathBuf::from("/tmp")),
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let result = Config::resolve(raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_blog_from_path() {
        let config = Config {
            blogs: HashMap::from([
                (
                    "blog1.example.com".to_string(),
                    ResolvedBlogConfig {
                        username: "u".to_string(),
                        password: "p".to_string(),
                        local_root: PathBuf::from("/tmp/blogs"),
                        omit_domain: false,
                        owner: None,
                    },
                ),
                (
                    "blog2.example.com".to_string(),
                    ResolvedBlogConfig {
                        username: "u".to_string(),
                        password: "p".to_string(),
                        local_root: PathBuf::from("/tmp/blogs"),
                        omit_domain: false,
                        owner: None,
                    },
                ),
            ]),
        };

        // Matches blog1 via domain prefix
        let (domain, _) = config
            .detect_blog_from_path(Path::new(
                "/tmp/blogs/blog1.example.com/entry/2024/01/01/test.md",
            ))
            .unwrap();
        assert_eq!(domain, "blog1.example.com");

        // Matches blog2 via domain prefix
        let (domain, _) = config
            .detect_blog_from_path(Path::new("/tmp/blogs/blog2.example.com/entry/my-post.md"))
            .unwrap();
        assert_eq!(domain, "blog2.example.com");

        // No match
        let result = config.detect_blog_from_path(Path::new("/other/path/file.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_blog_from_path_omit_domain() {
        let config = Config {
            blogs: HashMap::from([(
                "blog.example.com".to_string(),
                ResolvedBlogConfig {
                    username: "u".to_string(),
                    password: "p".to_string(),
                    local_root: PathBuf::from("/tmp/blog"),
                    omit_domain: true,
                    owner: None,
                },
            )]),
        };

        let (domain, _) = config
            .detect_blog_from_path(Path::new("/tmp/blog/entry/2024/01/01/test.md"))
            .unwrap();
        assert_eq!(domain, "blog.example.com");
    }

    #[test]
    fn test_resolve_missing_local_root() {
        let raw = RawConfig {
            default: None,
            blogs: HashMap::from([(
                "example.hatenablog.com".to_string(),
                BlogConfig {
                    username: Some("user".to_string()),
                    password: Some("pass".to_string()),
                    local_root: None,
                    omit_domain: None,
                    owner: None,
                },
            )]),
        };

        let result = Config::resolve(raw);
        assert!(result.is_err());
    }
}
