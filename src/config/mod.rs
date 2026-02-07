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
        let config_path = Self::find_config_path(workdir)?;
        let content =
            std::fs::read_to_string(&config_path).context("Failed to read config file")?;
        let raw: RawConfig =
            serde_yml::from_str(&content).context("Failed to parse config file")?;
        Self::resolve(raw)
    }

    fn find_config_path(workdir: Option<&Path>) -> Result<PathBuf> {
        let candidates = vec![
            workdir.map(|w| w.join("blogsync.yaml")),
            Some(PathBuf::from("blogsync.yaml")),
            dirs::config_dir().map(|d| d.join("blogsync").join("config.yaml")),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        anyhow::bail!(
            "Config file not found. Create blogsync.yaml or ~/.config/blogsync/config.yaml"
        )
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

            let omit_domain = blog
                .omit_domain
                .or(default.omit_domain)
                .unwrap_or(false);

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
