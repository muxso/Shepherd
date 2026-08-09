use crate::client::*;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum AuthCmd {
    /// Log in with username + password; prints the session token (separate from the API key used by `login`).
    Login {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// Invalidate the current session (requires the API key / bearer auth).
    Logout,
    /// Rotate the session token using the current one (requires bearer auth).
    Refresh,
    /// Show the current session user and permissions (requires bearer auth).
    Me,
    /// Change the current user's password (requires bearer auth).
    Password {
        #[arg(long = "old")]
        old_password: String,
        #[arg(long = "new")]
        new_password: String,
    },
    /// Print the OIDC authorize URL for a provider (open it in a browser to finish login).
    Oidc {
        #[arg(long)]
        provider: String,
    },
}

pub fn run(cmd: AuthCmd) -> R<()> {
    let cfg = Config::load();
    match cmd {
        AuthCmd::Login { username, password } => {
            let c = Client::new(cfg)?;
            let v =
                c.post("/auth/login", json!({"username": username, "password": password}), false)?;
            println!(
                " login success, session token:\n{}",
                v.get("token").and_then(|t| t.as_str()).unwrap_or("")
            );
            pretty(&v);
        }
        AuthCmd::Logout => {
            let c = Client::new(cfg)?;
            c.post("/auth/logout", json!({}), true)?;
            println!(" logged out");
        }
        AuthCmd::Refresh => {
            let c = Client::new(cfg)?;
            pretty(&c.post("/auth/refresh", json!({}), true)?);
        }
        AuthCmd::Me => {
            let c = Client::new(cfg)?;
            pretty(&c.get("/auth/me", true)?);
        }
        AuthCmd::Password { old_password, new_password } => {
            let c = Client::new(cfg)?;
            c.post(
                "/auth/password",
                json!({"oldPassword": old_password, "newPassword": new_password}),
                true,
            )?;
            println!(" password changed");
        }
        AuthCmd::Oidc { provider } => {
            let base = cfg.url.trim_end_matches('/');
            println!("{base}/auth/oidc/{provider}/authorize");
        }
    };
    Ok(())
}
