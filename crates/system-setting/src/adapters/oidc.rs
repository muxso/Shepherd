//! 飞书/企业微信是 OAuth2(非标准 OIDC),多步换取用户信息;端点路径按厂商文档实现,可能随厂商调整。

use async_trait::async_trait;
use serde::Deserialize;

use crate::domain::{ExternalIdentity, OidcError};
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
            .get(format!("{}/cgi-bin/auth/getuserinfo?access_token={token}&code={code}", self.base_url))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::{get, post}, Json, Router};
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
            .route("/cgi-bin/gettoken", get(|| async { Json(serde_json::json!({"errcode":0,"access_token":"qy-tok"})) }))
            .route("/cgi-bin/auth/getuserinfo", get(|| async { Json(serde_json::json!({"errcode":0,"userid":"zhangsan"})) }))
            .route("/cgi-bin/user/get", get(|| async { Json(serde_json::json!({"errcode":0,"name":"张三"})) }));
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
        assert!(url.contains("appid=corp1") && url.contains("state=st2") && url.contains("#wechat_redirect"));
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
}
