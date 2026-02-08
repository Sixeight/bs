use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha1::{Digest, Sha1};
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

pub fn generate(username: &str, password: &str) -> String {
    let mut nonce_bytes = [0u8; 20];
    getrandom::fill(&mut nonce_bytes).expect("Failed to generate nonce");
    let nonce_base64 = BASE64.encode(nonce_bytes);

    let now = OffsetDateTime::now_utc();
    let created = now
        .format(&Iso8601::DEFAULT)
        .expect("Failed to format time");

    let mut hasher = Sha1::new();
    hasher.update(nonce_bytes);
    hasher.update(created.as_bytes());
    hasher.update(password.as_bytes());
    let digest = BASE64.encode(hasher.finalize());

    format!(
        r#"UsernameToken Username="{}", PasswordDigest="{}", Nonce="{}", Created="{}""#,
        username, digest, nonce_base64, created
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_format() {
        let header = generate("testuser", "testpass");
        assert!(header.starts_with("UsernameToken Username=\"testuser\""));
        assert!(header.contains("PasswordDigest=\""));
        assert!(header.contains("Nonce=\""));
        assert!(header.contains("Created=\""));
    }

    #[test]
    fn test_generate_different_nonce_each_call() {
        let h1 = generate("user", "pass");
        let h2 = generate("user", "pass");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_generate_contains_valid_base64() {
        let header = generate("user", "pass");
        let nonce_start = header.find("Nonce=\"").unwrap() + 7;
        let nonce_end = header[nonce_start..].find('"').unwrap() + nonce_start;
        let nonce = &header[nonce_start..nonce_end];
        assert!(BASE64.decode(nonce).is_ok());

        let digest_start = header.find("PasswordDigest=\"").unwrap() + 16;
        let digest_end = header[digest_start..].find('"').unwrap() + digest_start;
        let digest = &header[digest_start..digest_end];
        assert!(BASE64.decode(digest).is_ok());
    }
}
