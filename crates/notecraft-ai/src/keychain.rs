//! The Claude API key lives in the macOS Keychain (service "NoteCraft",
//! account "claude-api-key"): never in the workspace, in settings files or
//! in git. On other systems the key can only come from the
//! `ANTHROPIC_API_KEY` environment variable.

pub const SERVICE: &str = "NoteCraft";
pub const ACCOUNT: &str = "claude-api-key";
pub const OPENAI_ACCOUNT: &str = "openai-api-key";
pub const GROK_ACCOUNT: &str = "grok-api-key";

/// The services that need a key; each has its own Keychain item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    OpenAi,
    Grok,
}

impl Provider {
    fn account(self) -> &'static str {
        match self {
            Provider::Claude => ACCOUNT,
            Provider::OpenAi => OPENAI_ACCOUNT,
            Provider::Grok => GROK_ACCOUNT,
        }
    }

    /// The environment variable read when nothing is stored.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Claude => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Grok => "XAI_API_KEY",
        }
    }
}

/// Store the Claude key (replacing any previous one).
pub fn save_api_key(key: &str) -> Result<(), String> {
    save_key(Provider::Claude, key)
}

/// The stored Claude key, or `ANTHROPIC_API_KEY`.
pub fn load_api_key() -> Option<String> {
    load_key(Provider::Claude)
}

/// True when a Claude key is stored in the Keychain (not counting the environment variable).
pub fn has_stored_key() -> bool {
    has_key(Provider::Claude)
}

/// Remove the stored Claude key.
pub fn delete_api_key() -> Result<(), String> {
    delete_key(Provider::Claude)
}

/// Store the key (replacing any previous one).
pub fn save_key(p: Provider, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("the key is empty".into());
    }
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, p.account(), key.as_bytes())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(format!("the Keychain is only available on macOS; set {} instead", p.env_var()))
    }
}

/// The stored key, or the provider's environment variable.
pub fn load_key(p: Provider) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(bytes) = security_framework::passwords::get_generic_password(SERVICE, p.account()) {
            if let Ok(s) = String::from_utf8(bytes) {
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
        }
    }
    std::env::var(p.env_var()).ok().filter(|k| !k.trim().is_empty())
}

/// True when a key is stored in the Keychain (not counting the environment variable).
/// Only the item's attributes are asked for, never the secret, so macOS shows no prompt.
pub fn has_key(p: Provider) -> bool {
    has_item(p.account())
}

/// True when a key is stored or given by the environment, without reading the secret.
pub fn is_available(p: Provider) -> bool {
    has_key(p) || std::env::var(p.env_var()).is_ok_and(|k| !k.trim().is_empty())
}

/// True when this Keychain item exists. The secret is not read, so there is no password prompt.
pub fn has_item(account: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(account)
            .limit(Limit::Max(1))
            .load_attributes(true)
            .search()
            .is_ok_and(|found| !found.is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        false
    }
}

/// Remove the stored key.
pub fn delete_key(p: Provider) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match security_framework::passwords::delete_generic_password(SERVICE, p.account()) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()), // errSecItemNotFound
            Err(e) => Err(e.to_string()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = p;
        Ok(())
    }
}

/// Keychain items for publishing a page (a Cloudflare account of the user's own).
pub const PUBLISH_TOKEN_ACCOUNT: &str = "cloudflare-api-token";
pub const PUBLISH_ACCOUNT_ID: &str = "cloudflare-account-id";

/// Store one Keychain item under the NoteCraft service (replacing any previous value).
pub fn save_item(account: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("the value is empty".into());
    }
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, account, value.as_bytes())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        Err("the Keychain is only available on macOS".into())
    }
}

/// One stored Keychain item, if any.
pub fn load_item(account: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let bytes = security_framework::passwords::get_generic_password(SERVICE, account).ok()?;
        String::from_utf8(bytes).ok().filter(|s| !s.trim().is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        None
    }
}

/// Remove one Keychain item (nothing to remove is fine).
pub fn delete_item(account: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match security_framework::passwords::delete_generic_password(SERVICE, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = account;
        Ok(())
    }
}

/// A Cloudflare account ID is 32 hexadecimal characters.
pub fn valid_cloudflare_account_id(s: &str) -> bool {
    let s = s.trim();
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// "sk-ant-…a1b2": enough to recognise a key without showing it.
pub fn masked(key: &str) -> String {
    let k = key.trim();
    let n = k.chars().count();
    if n <= 10 {
        return "•".repeat(n.min(8));
    }
    let head: String = k.chars().take(7).collect();
    let tail: String = k.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_hides_the_key() {
        assert_eq!(masked("sk-ant-api03-abcdefghijklmnop-1234"), "sk-ant-…1234");
        assert_eq!(masked("short"), "•••••");
        assert!(save_api_key("   ").is_err());
        assert!(save_key(Provider::OpenAi, "  ").is_err());
    }

    #[test]
    fn cloudflare_account_ids_are_32_hex_characters() {
        assert!(valid_cloudflare_account_id("0123456789abcdef0123456789ABCDEF"));
        assert!(!valid_cloudflare_account_id("0123456789abcdef"));
        assert!(!valid_cloudflare_account_id("zzzz456789abcdef0123456789abcdef"));
        assert!(save_item(PUBLISH_TOKEN_ACCOUNT, "  ").is_err());
    }

    #[test]
    fn each_provider_has_its_own_item_and_variable() {
        assert_ne!(Provider::Claude.account(), Provider::OpenAi.account());
        assert_eq!(Provider::OpenAi.env_var(), "OPENAI_API_KEY");
        assert_eq!(Provider::Claude.env_var(), "ANTHROPIC_API_KEY");
    }
}
