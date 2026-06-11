use crate::{ApiError, http_status_error, new_correlation_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auth0DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri_complete: String,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth0TokenPoll {
    Authorized { access_token: String },
    AuthorizationPending,
    SlowDown,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalAccessToken {
    pub key: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Auth0Client {
    client: reqwest::blocking::Client,
    base_url: String,
    client_id: String,
}

#[derive(Debug, Clone)]
pub struct JupiterClient {
    client: reqwest::blocking::Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct Auth0DeviceCodeRequest<'a> {
    client_id: &'a str,
    scope: &'a str,
    audience: &'a str,
}

#[derive(Debug, Deserialize)]
struct Auth0DeviceCodeResponse {
    device_code: String,
    user_code: String,
    #[serde(alias = "verification_uri")]
    verification_uri_complete: String,
    #[serde(default = "default_auth0_poll_interval")]
    interval: u64,
}

#[derive(Debug, Serialize)]
struct Auth0TokenRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    device_code: &'a str,
}

#[derive(Debug, Deserialize)]
struct Auth0TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreatePersonalAccessTokenRequest<'a> {
    name: &'a str,
}

#[derive(Debug, Deserialize)]
struct CreatePersonalAccessTokenResponse {
    key: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PersonalAccessTokensResponse {
    List(Vec<PersonalAccessTokenResponse>),
    Object {
        #[serde(default, alias = "personalAccessTokens")]
        pats: Vec<PersonalAccessTokenResponse>,
    },
}

#[derive(Debug, Deserialize)]
struct PersonalAccessTokenResponse {
    key: String,
    name: Option<String>,
}

impl Auth0Client {
    pub fn new(region: &str) -> Result<Self, ApiError> {
        let details = auth0_region_details(region)?;
        Ok(Self::with_base_url(details.base_url, details.client_id))
    }

    pub fn with_base_url(base_url: impl Into<String>, client_id: impl Into<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client_id: client_id.into(),
        }
    }

    pub fn request_device_code(&self) -> Result<Auth0DeviceCode, ApiError> {
        let url = format!("{}/oauth/device/code", self.base_url);
        let correlation_id = new_correlation_id();
        let response = self
            .client
            .post(&url)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .json(&Auth0DeviceCodeRequest {
                client_id: &self.client_id,
                scope: "openid profile email",
                audience: "https://platform.polyai.app/api",
            })
            .send()
            .map_err(|error| ApiError::Http(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        let body: Auth0DeviceCodeResponse = response
            .json()
            .map_err(|error| ApiError::Http(error.to_string()))?;
        parse_device_code(body)
    }

    pub fn poll_device_token(&self, device_code: &str) -> Result<Auth0TokenPoll, ApiError> {
        let url = format!("{}/oauth/token", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&Auth0TokenRequest {
                client_id: &self.client_id,
                grant_type: "urn:ietf:params:oauth:grant-type:device_code",
                device_code,
            })
            .send()
            .map_err(|error| ApiError::Http(error.to_string()))?;
        let status = response.status();
        let body: Auth0TokenResponse = response
            .json()
            .map_err(|error| ApiError::Http(error.to_string()))?;

        if status.is_success() {
            let access_token = body
                .access_token
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ApiError::Http("missing access_token in Auth0 response".to_string())
                })?;
            return Ok(Auth0TokenPoll::Authorized { access_token });
        }

        match body.error.as_deref() {
            Some("authorization_pending") => Ok(Auth0TokenPoll::AuthorizationPending),
            Some("slow_down") => Ok(Auth0TokenPoll::SlowDown),
            Some("expired_token") => Ok(Auth0TokenPoll::Expired),
            _ => Err(ApiError::Http(format!("status={status} body={body:?}"))),
        }
    }
}

impl JupiterClient {
    pub fn new(region: &str) -> Result<Self, ApiError> {
        Ok(Self::with_base_url(jupiter_base_url(region)?))
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn authorise(&self, jwt_access_token: &str) -> Result<Value, ApiError> {
        self.request_json(
            reqwest::Method::GET,
            "/jupiter/v1/authorise",
            jwt_access_token,
            None,
        )
    }

    pub fn list_personal_access_tokens(
        &self,
        jwt_access_token: &str,
    ) -> Result<Vec<PersonalAccessToken>, ApiError> {
        let body = self.request_json(
            reqwest::Method::GET,
            "/jupiter/v2/pats",
            jwt_access_token,
            None,
        )?;
        Ok(parse_personal_access_tokens(&body))
    }

    pub fn create_personal_access_token(
        &self,
        jwt_access_token: &str,
        name: &str,
    ) -> Result<String, ApiError> {
        let body = self.request_json(
            reqwest::Method::POST,
            "/jupiter/v2/pats",
            jwt_access_token,
            Some(
                serde_json::to_value(CreatePersonalAccessTokenRequest { name })
                    .map_err(|error| ApiError::Http(error.to_string()))?,
            ),
        )?;
        let response: CreatePersonalAccessTokenResponse =
            serde_json::from_value(body).map_err(|error| {
                ApiError::Http(format!("failed to parse PAT creation response: {error}"))
            })?;
        if response.key.is_empty() {
            return Err(ApiError::Http(
                "missing key in PAT creation response".to_string(),
            ));
        }
        Ok(response.key)
    }

    fn request_json(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        jwt_access_token: &str,
        body: Option<Value>,
    ) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let correlation_id = new_correlation_id();
        let mut request = self
            .client
            .request(method, &url)
            .bearer_auth(jwt_access_token)
            .header("X-PolyAI-Correlation-Id", &correlation_id)
            .header("Content-Type", "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|error| ApiError::Http(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(http_status_error(status, &url, &correlation_id));
        }
        response
            .json()
            .map_err(|error| ApiError::Http(error.to_string()))
    }
}

struct Auth0RegionDetails {
    base_url: &'static str,
    client_id: &'static str,
}

fn auth0_region_details(region: &str) -> Result<Auth0RegionDetails, ApiError> {
    let details = match region {
        "studio" => Auth0RegionDetails {
            base_url: "https://login.studio.poly.ai",
            client_id: "6uLCbsn6UxXJlnGKE4ypqwQqt3UqTUnd",
        },
        "us-1" => Auth0RegionDetails {
            base_url: "https://login.us-1.polyai.app",
            client_id: "kjV5BoAXagNnK6aGiUnJQ6xJ3hbphwE2",
        },
        "uk-1" => Auth0RegionDetails {
            base_url: "https://login.uk-1.polyai.app",
            client_id: "uHdlq2JZZoZ3RAzYDvl0o0R2E3glxj1q",
        },
        "euw-1" => Auth0RegionDetails {
            base_url: "https://login.euw-1.polyai.app",
            client_id: "SjDXzApQAQ9TkJHSDlASLkw7lBOe2QA4",
        },
        "dev" => Auth0RegionDetails {
            base_url: "https://login.dev.polyai.app",
            client_id: "xkHpmZsOmINkW5Khe406tHJPm9XkDVdf",
        },
        "staging" => Auth0RegionDetails {
            base_url: "https://login.staging.polyai.app",
            client_id: "lnq8WDCLLJ5uacDIrhnOFQAkohg6PJkB",
        },
        _ => return Err(ApiError::MissingConfig(format!("Unknown region: {region}"))),
    };
    Ok(details)
}

fn jupiter_base_url(region: &str) -> Result<&'static str, ApiError> {
    match region {
        "euw-1" => Ok("https://jupiter-api.euw-1.platform.polyai.app"),
        "uk-1" => Ok("https://jupiter-api.uk-1.platform.polyai.app"),
        "us-1" => Ok("https://jupiter-api.us-1.platform.polyai.app"),
        "dev" => Ok("https://jupiter-api.dev.polyai.app"),
        "staging" => Ok("https://jupiter-api.staging.us-1.platform.polyai.app"),
        "studio" => Ok("https://jupiter-api.plg-us-1-prod.polyai.app"),
        _ => Err(ApiError::MissingConfig(format!("Unknown region: {region}"))),
    }
}

fn parse_device_code(body: Auth0DeviceCodeResponse) -> Result<Auth0DeviceCode, ApiError> {
    if body.device_code.is_empty() {
        return Err(ApiError::Http(
            "missing device_code in Auth0 device-code response".to_string(),
        ));
    }
    if body.user_code.is_empty() {
        return Err(ApiError::Http(
            "missing user_code in Auth0 device-code response".to_string(),
        ));
    }
    if body.verification_uri_complete.is_empty() {
        return Err(ApiError::Http(
            "missing verification_uri_complete in Auth0 device-code response".to_string(),
        ));
    }
    Ok(Auth0DeviceCode {
        device_code: body.device_code,
        user_code: body.user_code,
        verification_uri_complete: body.verification_uri_complete,
        interval_seconds: body.interval,
    })
}

fn parse_personal_access_tokens(body: &Value) -> Vec<PersonalAccessToken> {
    let Ok(response) = serde_json::from_value::<PersonalAccessTokensResponse>(body.clone()) else {
        return vec![];
    };
    let values = match response {
        PersonalAccessTokensResponse::List(values) => values,
        PersonalAccessTokensResponse::Object { pats } => pats,
    };
    values
        .into_iter()
        .filter_map(|value| {
            if value.key.is_empty() {
                return None;
            }
            Some(PersonalAccessToken {
                key: value.key,
                name: value.name,
            })
        })
        .collect()
}

fn default_auth0_poll_interval() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use serde_json::json;

    #[test]
    fn auth0_device_code_request_matches_python_payload() {
        let server = MockServer::start();
        let auth0 = Auth0Client::with_base_url(server.base_url(), "client-id");
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/oauth/device/code")
                .json_body(json!({
                    "client_id": "client-id",
                    "scope": "openid profile email",
                    "audience": "https://platform.polyai.app/api",
                }));
            then.status(200).json_body(json!({
                "device_code": "device-code",
                "user_code": "ABCD-EFGH",
                "verification_uri_complete": "https://example.test/device",
                "interval": 2,
            }));
        });

        let code = auth0.request_device_code().expect("device code");

        assert_eq!(
            code,
            Auth0DeviceCode {
                device_code: "device-code".to_string(),
                user_code: "ABCD-EFGH".to_string(),
                verification_uri_complete: "https://example.test/device".to_string(),
                interval_seconds: 2,
            }
        );
        mock.assert_calls(1);
    }

    #[test]
    fn auth0_token_poll_maps_pending_slow_down_expired_and_success() {
        for (error, expected) in [
            (
                "authorization_pending",
                Auth0TokenPoll::AuthorizationPending,
            ),
            ("slow_down", Auth0TokenPoll::SlowDown),
            ("expired_token", Auth0TokenPoll::Expired),
        ] {
            let server = MockServer::start();
            let auth0 = Auth0Client::with_base_url(server.base_url(), "client-id");
            server.mock(|when, then| {
                when.method(POST).path("/oauth/token").json_body(json!({
                    "client_id": "client-id",
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                    "device_code": "device-code",
                }));
                then.status(403).json_body(json!({ "error": error }));
            });

            assert_eq!(
                auth0.poll_device_token("device-code").expect("poll result"),
                expected
            );
        }

        let server = MockServer::start();
        let auth0 = Auth0Client::with_base_url(server.base_url(), "client-id");
        server.mock(|when, then| {
            when.method(POST).path("/oauth/token");
            then.status(200)
                .json_body(json!({ "access_token": "jwt-token" }));
        });

        assert_eq!(
            auth0.poll_device_token("device-code").expect("token"),
            Auth0TokenPoll::Authorized {
                access_token: "jwt-token".to_string(),
            }
        );
    }

    #[test]
    fn jupiter_pat_flow_uses_bearer_auth_and_expected_paths() {
        let server = MockServer::start();
        let jupiter = JupiterClient::with_base_url(server.base_url());
        let authorise = server.mock(|when, then| {
            when.method(GET)
                .path("/jupiter/v1/authorise")
                .header("authorization", "Bearer jwt-token");
            then.status(200).json_body(json!({ "ok": true }));
        });
        let list = server.mock(|when, then| {
            when.method(GET)
                .path("/jupiter/v2/pats")
                .header("authorization", "Bearer jwt-token");
            then.status(200).json_body(json!([
                { "name": "adk-key", "key": "existing-key" }
            ]));
        });
        let create = server.mock(|when, then| {
            when.method(POST)
                .path("/jupiter/v2/pats")
                .header("authorization", "Bearer jwt-token")
                .json_body(json!({ "name": "adk-key" }));
            then.status(200).json_body(json!({ "key": "created-key" }));
        });

        assert_eq!(
            jupiter.authorise("jwt-token").expect("authorise"),
            json!({ "ok": true })
        );
        assert_eq!(
            jupiter
                .list_personal_access_tokens("jwt-token")
                .expect("list pats"),
            vec![PersonalAccessToken {
                key: "existing-key".to_string(),
                name: Some("adk-key".to_string()),
            }]
        );
        assert_eq!(
            jupiter
                .create_personal_access_token("jwt-token", "adk-key")
                .expect("create pat"),
            "created-key"
        );
        authorise.assert_calls(1);
        list.assert_calls(1);
        create.assert_calls(1);
    }
}
