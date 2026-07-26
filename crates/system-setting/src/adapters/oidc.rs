//! Feishu/WeCom are OAuth2 (not standard OIDC) and need multiple steps to obtain user info;
//! endpoint paths follow vendor docs and may change on their side.

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

use crate::domain::{ExternalIdentity, OidcError, OidcProvider};
use crate::ports::ExternalIdentityProvider;

fn ex(e: impl std::fmt::Display) -> OidcError {
    OidcError::Exchange(e.to_string())
}

#[derive(Clone)]
pub struct FeishuProvider {
    client: reqwest::Client,
    app_id: String,
    app_secret: String,
    redirect_uri: String,
    base_url: String,
}

impl FeishuProvider {
    pub fn new(app_id: &str, app_secret: &str, redirect_uri: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            base_url: "https://open.feishu.cn".to_string(),
        }
    }

    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl ExternalIdentityProvider for FeishuProvider {
    fn key(&self) -> &str {
        "feishu"
    }

    fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/open-apis/authen/v1/authorize?app_id={}&redirect_uri={}&state={state}",
            self.base_url, self.app_id, self.redirect_uri
        )
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        #[derive(Deserialize)]
        struct AppTok {
            code: i64,
            app_access_token: Option<String>,
        }
        let at: AppTok = self
            .client
            .post(format!("{}/open-apis/auth/v3/app_access_token/internal", self.base_url))
            .json(&serde_json::json!({"app_id": self.app_id, "app_secret": self.app_secret}))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if at.code != 0 {
            return Err(OidcError::Exchange(format!("feishu app_access_token code={}", at.code)));
        }
        let app_token = at.app_access_token.ok_or_else(|| ex("missing app_access_token"))?;

        #[derive(Deserialize)]
        struct Data {
            open_id: String,
            name: String,
        }
        #[derive(Deserialize)]
        struct UserResp {
            code: i64,
            data: Option<Data>,
        }
        let u: UserResp = self
            .client
            .post(format!("{}/open-apis/authen/v1/access_token", self.base_url))
            .bearer_auth(&app_token)
            .json(&serde_json::json!({"grant_type": "authorization_code", "code": code}))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if u.code != 0 {
            return Err(OidcError::Exchange(format!("feishu access_token code={}", u.code)));
        }
        let d = u.data.ok_or_else(|| ex("missing user data"))?;
        Ok(ExternalIdentity { provider: "feishu".into(), open_id: d.open_id, display_name: d.name })
    }
}

#[derive(Clone)]
pub struct WecomProvider {
    client: reqwest::Client,
    corp_id: String,
    corp_secret: String,
    redirect_uri: String,
    base_url: String,
    authorize_base: String,
}

impl WecomProvider {
    pub fn new(corp_id: &str, corp_secret: &str, redirect_uri: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            corp_id: corp_id.to_string(),
            corp_secret: corp_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            base_url: "https://qyapi.weixin.qq.com".to_string(),
            authorize_base: "https://open.weixin.qq.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base: &str) -> Self {
        let b = base.trim_end_matches('/').to_string();
        self.base_url = b.clone();
        self.authorize_base = b;
        self
    }
}

#[async_trait]
impl ExternalIdentityProvider for WecomProvider {
    fn key(&self) -> &str {
        "wecom"
    }

    fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/connect/oauth2/authorize?appid={}&redirect_uri={}&response_type=code&scope=snsapi_base&state={state}#wechat_redirect",
            self.authorize_base, self.corp_id, self.redirect_uri
        )
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        #[derive(Deserialize)]
        struct Tok {
            errcode: i64,
            access_token: Option<String>,
        }
        let t: Tok = self
            .client
            .get(format!(
                "{}/cgi-bin/gettoken?corpid={}&corpsecret={}",
                self.base_url, self.corp_id, self.corp_secret
            ))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if t.errcode != 0 {
            return Err(OidcError::Exchange(format!("wecom gettoken errcode={}", t.errcode)));
        }
        let token = t.access_token.ok_or_else(|| ex("missing access_token"))?;

        #[derive(Deserialize)]
        struct UInfo {
            errcode: i64,
            userid: Option<String>,
        }
        let ui: UInfo = self
            .client
            .get(format!(
                "{}/cgi-bin/auth/getuserinfo?access_token={token}&code={code}",
                self.base_url
            ))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if ui.errcode != 0 {
            return Err(OidcError::Exchange(format!("wecom getuserinfo errcode={}", ui.errcode)));
        }
        let userid = ui.userid.ok_or_else(|| ex("missing userid"))?;

        #[derive(Deserialize)]
        struct UDetail {
            errcode: i64,
            name: Option<String>,
        }
        let ud: UDetail = self
            .client
            .get(format!("{}/cgi-bin/user/get?access_token={token}&userid={userid}", self.base_url))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        let name = if ud.errcode == 0 { ud.name } else { None }.unwrap_or_else(|| userid.clone());
        Ok(ExternalIdentity { provider: "wecom".into(), open_id: userid, display_name: name })
    }
}

// ---- Lark: international Feishu. Identical OAuth2 flow, different base host. ----

#[derive(Clone)]
pub struct LarkProvider {
    client: reqwest::Client,
    app_id: String,
    app_secret: String,
    redirect_uri: String,
    base_url: String,
}

impl LarkProvider {
    pub fn new(app_id: &str, app_secret: &str, redirect_uri: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            base_url: "https://open.larksuite.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl ExternalIdentityProvider for LarkProvider {
    fn key(&self) -> &str {
        "lark"
    }

    fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/open-apis/authen/v1/authorize?app_id={}&redirect_uri={}&state={state}",
            self.base_url, self.app_id, self.redirect_uri
        )
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        // Lark reuses Feishu's OpenAPI OAuth2 endpoints.
        #[derive(Deserialize)]
        struct AppTok {
            code: i64,
            app_access_token: Option<String>,
        }
        let at: AppTok = self
            .client
            .post(format!("{}/open-apis/auth/v3/app_access_token/internal", self.base_url))
            .json(&serde_json::json!({"app_id": self.app_id, "app_secret": self.app_secret}))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if at.code != 0 {
            return Err(OidcError::Exchange(format!("lark app_access_token code={}", at.code)));
        }
        let app_token = at.app_access_token.ok_or_else(|| ex("missing app_access_token"))?;

        #[derive(Deserialize)]
        struct Data {
            open_id: String,
            name: String,
        }
        #[derive(Deserialize)]
        struct UserResp {
            code: i64,
            data: Option<Data>,
        }
        let u: UserResp = self
            .client
            .post(format!("{}/open-apis/authen/v1/access_token", self.base_url))
            .bearer_auth(&app_token)
            .json(&serde_json::json!({"grant_type": "authorization_code", "code": code}))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if u.code != 0 {
            return Err(OidcError::Exchange(format!("lark access_token code={}", u.code)));
        }
        let d = u.data.ok_or_else(|| ex("missing user data"))?;
        Ok(ExternalIdentity { provider: "lark".into(), open_id: d.open_id, display_name: d.name })
    }
}

// ---- DingTalk: QR connect -> gettoken -> sns/getuserinfo_bycode. ----

#[derive(Clone)]
pub struct DingTalkProvider {
    client: reqwest::Client,
    app_key: String,
    app_secret: String,
    redirect_uri: String,
    base_url: String,
}

impl DingTalkProvider {
    pub fn new(app_key: &str, app_secret: &str, redirect_uri: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            app_key: app_key.to_string(),
            app_secret: app_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            base_url: "https://oapi.dingtalk.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl ExternalIdentityProvider for DingTalkProvider {
    fn key(&self) -> &str {
        "dingtalk"
    }

    fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/connect/qrconnect?appid={}&response_type=code&scope=snsapi_login&state={state}&redirect_uri={}",
            self.base_url, self.app_key, self.redirect_uri
        )
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        #[derive(Deserialize)]
        struct Tok {
            errcode: i64,
            access_token: Option<String>,
        }
        let t: Tok = self
            .client
            .get(format!(
                "{}/gettoken?appkey={}&appsecret={}",
                self.base_url, self.app_key, self.app_secret
            ))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if t.errcode != 0 {
            return Err(OidcError::Exchange(format!("dingtalk gettoken errcode={}", t.errcode)));
        }
        let token = t.access_token.ok_or_else(|| ex("missing access_token"))?;

        #[derive(Deserialize)]
        struct DingUser {
            nick: Option<String>,
            openid: Option<String>,
        }
        #[derive(Deserialize)]
        struct UserInfo {
            errcode: i64,
            user_info: Option<DingUser>,
        }
        let ui: UserInfo = self
            .client
            .post(format!("{}/sns/getuserinfo_bycode?access_token={token}", self.base_url))
            .json(&serde_json::json!({ "tmp_auth_code": code }))
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if ui.errcode != 0 {
            return Err(OidcError::Exchange(format!(
                "dingtalk getuserinfo errcode={}",
                ui.errcode
            )));
        }
        let u = ui.user_info.ok_or_else(|| ex("missing user_info"))?;
        let open_id = u.openid.ok_or_else(|| ex("missing openid"))?;
        let name = u.nick.unwrap_or_else(|| open_id.clone());
        Ok(ExternalIdentity { provider: "dingtalk".into(), open_id, display_name: name })
    }
}

// ---- Slack: standard OIDC (openid.connect.*). ----

#[derive(Clone)]
pub struct SlackProvider {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    base_url: String,
}

impl SlackProvider {
    pub fn new(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            base_url: "https://slack.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl ExternalIdentityProvider for SlackProvider {
    fn key(&self) -> &str {
        "slack"
    }

    fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/openid/connect/authorize?client_id={}&response_type=code&scope=openid+profile+email&state={state}&redirect_uri={}",
            self.base_url, self.client_id, self.redirect_uri
        )
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        #[derive(Deserialize)]
        struct TokenResp {
            ok: Option<bool>,
            error: Option<String>,
            access_token: Option<String>,
        }
        let t: TokenResp = self
            .client
            .post(format!("{}/api/openid.connect.token", self.base_url))
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("grant_type", "authorization_code"),
                ("redirect_uri", self.redirect_uri.as_str()),
            ])
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        if t.ok != Some(true) {
            return Err(OidcError::Exchange(
                t.error.unwrap_or_else(|| "slack token exchange failed".into()),
            ));
        }
        let token = t.access_token.ok_or_else(|| ex("missing access_token"))?;

        #[derive(Deserialize)]
        struct UInfo {
            sub: Option<String>,
            name: Option<String>,
        }
        let u: UInfo = self
            .client
            .get(format!("{}/api/openid.connect.userInfo", self.base_url))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(ex)?
            .json()
            .await
            .map_err(ex)?;
        let open_id = u.sub.ok_or_else(|| ex("missing sub"))?;
        let name = u.name.unwrap_or_else(|| open_id.clone());
        Ok(ExternalIdentity { provider: "slack".into(), open_id, display_name: name })
    }
}

/// Builds the strategy object for a stored [`OidcProvider`] config. Returns
/// `None` for unknown provider keys so they are silently skipped when loading
/// the runtime registry (instead of failing the whole load).
pub fn build_provider(p: &OidcProvider) -> Option<Arc<dyn ExternalIdentityProvider>> {
    fn with_opt_base<T: WithBaseUrl>(p: T, base: &Option<String>) -> T {
        match base {
            Some(b) => p.with_base_url(b),
            None => p,
        }
    }
    match p.provider_key.as_str() {
        "feishu" => Some(Arc::new(with_opt_base(
            FeishuProvider::new(&p.app_id, &p.app_secret, &p.redirect),
            &p.base_url,
        ))),
        "wecom" => Some(Arc::new(with_opt_base(
            WecomProvider::new(&p.app_id, &p.app_secret, &p.redirect),
            &p.base_url,
        ))),
        "lark" => Some(Arc::new(with_opt_base(
            LarkProvider::new(&p.app_id, &p.app_secret, &p.redirect),
            &p.base_url,
        ))),
        "dingtalk" => Some(Arc::new(with_opt_base(
            DingTalkProvider::new(&p.app_id, &p.app_secret, &p.redirect),
            &p.base_url,
        ))),
        "slack" => Some(Arc::new(with_opt_base(
            SlackProvider::new(&p.app_id, &p.app_secret, &p.redirect),
            &p.base_url,
        ))),
        _ => None,
    }
}

/// Implemented by every provider so [`build_provider`] can apply an optional
/// `base_url` override uniformly.
pub trait WithBaseUrl {
    fn with_base_url(self, base: &str) -> Self;
}

impl WithBaseUrl for FeishuProvider {
    fn with_base_url(self, base: &str) -> Self {
        Self::with_base_url(self, base)
    }
}
impl WithBaseUrl for WecomProvider {
    fn with_base_url(self, base: &str) -> Self {
        Self::with_base_url(self, base)
    }
}
impl WithBaseUrl for LarkProvider {
    fn with_base_url(self, base: &str) -> Self {
        Self::with_base_url(self, base)
    }
}
impl WithBaseUrl for DingTalkProvider {
    fn with_base_url(self, base: &str) -> Self {
        Self::with_base_url(self, base)
    }
}
impl WithBaseUrl for SlackProvider {
    fn with_base_url(self, base: &str) -> Self {
        Self::with_base_url(self, base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    use tokio::net::TcpListener;

    async fn spawn_feishu() -> String {
        let app = Router::new()
            .route(
                "/open-apis/auth/v3/app_access_token/internal",
                post(|| async { Json(serde_json::json!({"code":0,"app_access_token":"app-tok"})) }),
            )
            .route(
                "/open-apis/authen/v1/access_token",
                post(|| async {
                    Json(serde_json::json!({"code":0,"data":{"open_id":"ou_alice","name":"Alice"}}))
                }),
            );
        serve(app).await
    }

    async fn spawn_wecom() -> String {
        let app = Router::new()
            .route(
                "/cgi-bin/gettoken",
                get(|| async { Json(serde_json::json!({"errcode":0,"access_token":"qy-tok"})) }),
            )
            .route(
                "/cgi-bin/auth/getuserinfo",
                get(|| async { Json(serde_json::json!({"errcode":0,"userid":"zhangsan"})) }),
            )
            .route(
                "/cgi-bin/user/get",
                get(|| async { Json(serde_json::json!({"errcode":0,"name":"张三"})) }),
            );
        serve(app).await
    }

    async fn serve(app: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn feishu_authorize_url_well_formed() {
        let p = FeishuProvider::new("cli_app", "sec", "https://ms/cb");
        let url = p.authorize_url("st1");
        assert!(url.contains("app_id=cli_app") && url.contains("state=st1"));
    }

    #[tokio::test]
    async fn feishu_exchange_parses_identity() {
        let base = spawn_feishu().await;
        let p = FeishuProvider::new("cli_app", "sec", "https://ms/cb").with_base_url(&base);
        let id = p.exchange("auth-code").await.expect("exchange");
        assert_eq!(id.provider, "feishu");
        assert_eq!(id.open_id, "ou_alice");
        assert_eq!(id.display_name, "Alice");
    }

    #[tokio::test]
    async fn wecom_authorize_url_well_formed() {
        let p = WecomProvider::new("corp1", "sec", "https://ms/cb");
        let url = p.authorize_url("st2");
        assert!(
            url.contains("appid=corp1")
                && url.contains("state=st2")
                && url.contains("#wechat_redirect")
        );
    }

    #[tokio::test]
    async fn wecom_exchange_parses_identity() {
        let base = spawn_wecom().await;
        let p = WecomProvider::new("corp1", "sec", "https://ms/cb").with_base_url(&base);
        let id = p.exchange("auth-code").await.expect("exchange");
        assert_eq!(id.provider, "wecom");
        assert_eq!(id.open_id, "zhangsan");
        assert_eq!(id.display_name, "张三");
    }

    async fn spawn_lark() -> String {
        let app = Router::new()
            .route(
                "/open-apis/auth/v3/app_access_token/internal",
                post(|| async { Json(serde_json::json!({"code":0,"app_access_token":"app-tok"})) }),
            )
            .route(
                "/open-apis/authen/v1/access_token",
                post(|| async {
                    Json(serde_json::json!({"code":0,"data":{"open_id":"ou_lark","name":"Larky"}}))
                }),
            );
        serve(app).await
    }

    async fn spawn_dingtalk() -> String {
        let app = Router::new()
            .route(
                "/gettoken",
                get(|| async { Json(serde_json::json!({"errcode":0,"access_token":"dt-tok"})) }),
            )
            .route(
                "/sns/getuserinfo_bycode",
                post(|| async {
                    Json(serde_json::json!({"errcode":0,"user_info":{"nick":"Li","openid":"dt_open_1"}}))
                }),
            );
        serve(app).await
    }

    async fn spawn_slack() -> String {
        let app = Router::new()
            .route(
                "/api/openid.connect.token",
                post(|| async { Json(serde_json::json!({"ok":true,"access_token":"slk-tok"})) }),
            )
            .route(
                "/api/openid.connect.userInfo",
                get(|| async { Json(serde_json::json!({"sub":"U1","name":"Bob"})) }),
            );
        serve(app).await
    }

    #[tokio::test]
    async fn lark_authorize_url_well_formed() {
        let p = LarkProvider::new("cli_app", "sec", "https://ms/cb");
        let url = p.authorize_url("st3");
        assert!(url.contains("app_id=cli_app") && url.contains("state=st3"));
        assert!(url.starts_with("https://open.larksuite.com"));
    }

    #[tokio::test]
    async fn lark_exchange_parses_identity() {
        let base = spawn_lark().await;
        let p = LarkProvider::new("cli_app", "sec", "https://ms/cb").with_base_url(&base);
        let id = p.exchange("auth-code").await.expect("exchange");
        assert_eq!(id.provider, "lark");
        assert_eq!(id.open_id, "ou_lark");
        assert_eq!(id.display_name, "Larky");
    }

    #[tokio::test]
    async fn dingtalk_authorize_url_well_formed() {
        let p = DingTalkProvider::new("dk1", "sec", "https://ms/cb");
        let url = p.authorize_url("st4");
        assert!(
            url.contains("appid=dk1")
                && url.contains("state=st4")
                && url.contains("qrconnect")
                && url.contains("snsapi_login")
        );
    }

    #[tokio::test]
    async fn dingtalk_exchange_parses_identity() {
        let base = spawn_dingtalk().await;
        let p = DingTalkProvider::new("dk1", "sec", "https://ms/cb").with_base_url(&base);
        let id = p.exchange("tmp-code").await.expect("exchange");
        assert_eq!(id.provider, "dingtalk");
        assert_eq!(id.open_id, "dt_open_1");
        assert_eq!(id.display_name, "Li");
    }

    #[tokio::test]
    async fn slack_authorize_url_well_formed() {
        let p = SlackProvider::new("sl_client", "sec", "https://ms/cb");
        let url = p.authorize_url("st5");
        assert!(
            url.contains("client_id=sl_client")
                && url.contains("state=st5")
                && url.contains("openid/connect/authorize")
                && url.contains("scope=openid")
        );
    }

    #[tokio::test]
    async fn slack_exchange_parses_identity() {
        let base = spawn_slack().await;
        let p = SlackProvider::new("sl_client", "sec", "https://ms/cb").with_base_url(&base);
        let id = p.exchange("auth-code").await.expect("exchange");
        assert_eq!(id.provider, "slack");
        assert_eq!(id.open_id, "U1");
        assert_eq!(id.display_name, "Bob");
    }

    #[tokio::test]
    async fn build_provider_known_keys_and_base_override() {
        let cfg = OidcProvider {
            provider_key: "lark".into(),
            app_id: "app".into(),
            app_secret: "sec".into(),
            redirect: "https://ms/cb".into(),
            default_permissions: vec!["PROJECT:READ".into()],
            enabled: true,
            base_url: Some("https://mock.local".into()),
        };
        let p = build_provider(&cfg).expect("built");
        assert_eq!(p.key(), "lark");

        // Unknown key yields None (so load can skip it).
        let unknown = OidcProvider { provider_key: "github".into(), ..cfg.clone() };
        assert!(build_provider(&unknown).is_none());
    }

    #[tokio::test]
    async fn build_provider_exchange_end_to_end() {
        let base = spawn_slack().await;
        let cfg = OidcProvider {
            provider_key: "slack".into(),
            app_id: "sl_client".into(),
            app_secret: "sec".into(),
            redirect: "https://ms/cb".into(),
            default_permissions: vec![],
            enabled: true,
            base_url: Some(base),
        };
        let p = build_provider(&cfg).expect("built");
        let id = p.exchange("auth-code").await.expect("exchange");
        assert_eq!(id.open_id, "U1");
    }
}
