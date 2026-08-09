use crate::client::*;
use clap::Subcommand;

/// Manage OIDC identity providers as runtime system settings.
///
/// Providers are stored server-side (DB) and drive the SSO buttons on the login
/// page. Mutations require SYSTEM_USER:UPDATE; listing requires SYSTEM_USER:READ.
#[derive(Subcommand)]
pub enum OidcAdminCmd {
    /// List configured OIDC providers (masks app secret).
    List,
    /// Show a single provider by key.
    Get {
        #[arg(long)]
        key: String,
    },
    /// Create a provider. `provider-key` selects the strategy
    /// (feishu|wecom|lark|dingtalk|slack).
    Create {
        #[arg(long = "provider-key")]
        provider_key: String,
        #[arg(long)]
        app_id: String,
        #[arg(long)]
        app_secret: String,
        /// Redirect/callback URI registered with the IdP.
        #[arg(long)]
        redirect: Option<String>,
        /// Optional custom base URL (e.g. an intranet gateway).
        #[arg(long = "base-url")]
        base_url: Option<String>,
        /// Default permissions granted to users provisioned via this provider
        /// (comma-separated; e.g. PROJECT:READ).
        #[arg(long, value_delimiter = ',')]
        default_permissions: Vec<String>,
        /// Disable the provider on creation.
        #[arg(long, default_value_t = false)]
        disabled: bool,
    },
    /// Update an existing provider. Omitted fields keep their current value;
    /// pass `--app-secret` empty to leave the secret unchanged.
    Update {
        #[arg(long = "provider-key")]
        provider_key: String,
        #[arg(long)]
        app_id: String,
        /// Leave empty to keep the existing secret.
        #[arg(long)]
        app_secret: Option<String>,
        #[arg(long)]
        redirect: Option<String>,
        #[arg(long = "base-url")]
        base_url: Option<String>,
        #[arg(long, value_delimiter = ',')]
        default_permissions: Vec<String>,
        /// Force enabled/disabled state (omit to keep current).
        #[arg(long)]
        enabled: Option<bool>,
    },
    /// Delete a provider by key.
    Delete {
        #[arg(long = "provider-key")]
        provider_key: String,
    },
}

pub fn run(cmd: OidcAdminCmd) -> R<()> {
    let c = Client::new(Config::load())?;
    match cmd {
        OidcAdminCmd::List => pretty(&c.get("/system/oidc/providers", true)?),
        OidcAdminCmd::Get { key } => {
            pretty(&c.get(&format!("/system/oidc/providers/{key}"), true)?)
        }
        OidcAdminCmd::Create {
            provider_key,
            app_id,
            app_secret,
            redirect,
            base_url,
            default_permissions,
            disabled,
        } => {
            let mut body = json!({
                "providerKey": provider_key,
                "appId": app_id,
                "appSecret": app_secret,
                "enabled": !disabled,
            });
            if let Some(r) = redirect {
                body["redirect"] = json!(r);
            }
            if let Some(b) = base_url {
                body["baseUrl"] = json!(b);
            }
            if !default_permissions.is_empty() {
                body["defaultPermissions"] = json!(default_permissions);
            }
            pretty(&c.post("/system/oidc/providers", body, true)?);
        }
        OidcAdminCmd::Update {
            provider_key,
            app_id,
            app_secret,
            redirect,
            base_url,
            default_permissions,
            enabled,
        } => {
            let mut body = json!({ "appId": app_id });
            if let Some(s) = app_secret {
                if !s.is_empty() {
                    body["appSecret"] = json!(s);
                }
            }
            if let Some(r) = redirect {
                body["redirect"] = json!(r);
            }
            if let Some(b) = base_url {
                body["baseUrl"] = json!(b);
            }
            if !default_permissions.is_empty() {
                body["defaultPermissions"] = json!(default_permissions);
            }
            if let Some(e) = enabled {
                body["enabled"] = json!(e);
            }
            pretty(&c.put(&format!("/system/oidc/providers/{provider_key}"), body, true)?);
        }
        OidcAdminCmd::Delete { provider_key } => {
            c.delete(&format!("/system/oidc/providers/{provider_key}"), true)?;
            println!(" deleted OIDC provider: {provider_key}");
        }
    }
    Ok(())
}
