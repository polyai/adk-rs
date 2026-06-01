use crate::{LoginArgs, console, credentials, init::INIT_REGIONS, prompt_select};
use adk_api_client::{
    Auth0Client, Auth0DeviceCode, Auth0TokenPoll, HttpPlatformClient, JupiterClient,
};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

const LOGIN_REGIONS: &[&str] = INIT_REGIONS;

pub(crate) fn cmd_login(args: LoginArgs) -> ExitCode {
    console::print_welcome_message();
    console::plain(
        "This will guide you through logging in to your Agent Studio account and setting up your API key for use with the ADK.",
    );
    if let Err(error) = crate::wait_for_enter("Press any key to continue...") {
        crate::emit_error(false, &error);
        return ExitCode::from(1);
    }

    let region = match resolve_login_region(args.region) {
        Ok(Some(region)) => region,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            crate::emit_error(false, &error);
            return ExitCode::from(1);
        }
    };
    match sign_in_and_save_key(&region) {
        Ok(_) => {
            console::success("Logged in successfully!");
            ExitCode::SUCCESS
        }
        Err(error) => {
            crate::emit_error(false, &error);
            ExitCode::from(1)
        }
    }
}

pub(crate) fn sign_in_and_save_key(region: &str) -> Result<String, String> {
    let jwt_access_token = sign_in(region)?;
    let api_key = authenticate_and_save_key(region, &jwt_access_token)?;
    wait_for_api_key_active(region, &api_key)?;
    Ok(api_key)
}

pub(crate) fn sign_in(region: &str) -> Result<String, String> {
    let auth0 = Auth0Client::new(region).map_err(|error| error.to_string())?;
    sign_in_with_client(&auth0, open_browser, thread::sleep)
}

trait DeviceAuthClient {
    fn request_device_code(&self) -> Result<Auth0DeviceCode, String>;
    fn poll_device_token(&self, device_code: &str) -> Result<Auth0TokenPoll, String>;
}

impl DeviceAuthClient for Auth0Client {
    fn request_device_code(&self) -> Result<Auth0DeviceCode, String> {
        Auth0Client::request_device_code(self)
            .map_err(|error| format!("Failed to request device code: {error}"))
    }

    fn poll_device_token(&self, device_code: &str) -> Result<Auth0TokenPoll, String> {
        Auth0Client::poll_device_token(self, device_code)
            .map_err(|error| format!("Failed to poll for authorization: {error}"))
    }
}

fn sign_in_with_client(
    auth0: &impl DeviceAuthClient,
    mut open_browser: impl FnMut(&str),
    mut sleep: impl FnMut(Duration),
) -> Result<String, String> {
    let device_code = auth0.request_device_code()?;

    console::info(format!(
        "To sign in or create an account, open the following link in your browser\nand enter the code when prompted.\n\n  URL:  {}\n  Code: [label]{}[/label]",
        device_code.verification_uri_complete, device_code.user_code
    ));
    open_browser(&device_code.verification_uri_complete);

    let mut interval = device_code.interval_seconds.max(1);
    loop {
        sleep(Duration::from_secs(interval));
        match auth0.poll_device_token(&device_code.device_code)? {
            Auth0TokenPoll::Authorized { access_token } => {
                console::success("Authenticated successfully!");
                return Ok(access_token);
            }
            Auth0TokenPoll::AuthorizationPending => {}
            Auth0TokenPoll::SlowDown => interval += 5,
            Auth0TokenPoll::Expired => {
                return Err("Authorization timed out. Please try again.".to_string());
            }
        }
    }
}

pub(crate) fn authenticate_and_save_key(
    region: &str,
    jwt_access_token: &str,
) -> Result<String, String> {
    let jupiter = JupiterClient::new(region).map_err(|error| error.to_string())?;
    console::info("Setting up your account...");
    jupiter
        .authorise(jwt_access_token)
        .map_err(|error| format!("Failed to authorize account: {error}"))?;

    console::info("Fetching API key...");
    let api_key = match jupiter
        .list_personal_access_tokens(jwt_access_token)
        .map_err(|error| format!("Failed to list API keys: {error}"))?
        .into_iter()
        .next()
    {
        Some(token) => {
            console::success(format!(
                "Found existing API Token: {}",
                credentials::mask_api_key(&token.key)
            ));
            token.key
        }
        None => {
            console::info("No existing API key found in your account.");
            console::info("Creating a new API key...");
            let key = jupiter
                .create_personal_access_token(jwt_access_token, "adk-key")
                .map_err(|error| format!("Failed to create API key: {error}"))?;
            console::success(format!(
                "Created a new API Key: {}",
                credentials::mask_api_key(&key)
            ));
            key
        }
    };

    let path = credentials::save_api_key_credential_file(&api_key, region)?;
    console::plain("API key has been saved to your credential file for future use.");
    console::info(format!("Credential file path: {}", path.display()));
    console::plain("");
    Ok(api_key)
}

pub(crate) fn wait_for_api_key_active(region: &str, api_key: &str) -> Result<(), String> {
    for _ in 0..20 {
        if HttpPlatformClient::list_accounts_with_api_key(region, api_key)
            .is_ok_and(|accounts| !accounts.is_empty())
        {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err("Timed out waiting for the API key to become active.".to_string())
}

fn resolve_login_region(region: Option<String>) -> Result<Option<String>, String> {
    match region {
        Some(region) => Ok(Some(region)),
        None => {
            let selected = prompt_select("Select your region:", &login_region_choices())?;
            if selected.is_none() {
                console::warning("No region selected. Exiting.");
            }
            Ok(selected)
        }
    }
}

fn login_region_choices() -> Vec<(String, String)> {
    let labels = [
        ("studio", "Studio"),
        ("us-1", "US (us-1) — Enterprise"),
        ("uk-1", "UK (uk-1) — Enterprise"),
        ("euw-1", "EU West (euw-1) — Enterprise"),
    ];
    labels
        .into_iter()
        .filter(|(region, _)| LOGIN_REGIONS.contains(region))
        .map(|(region, label)| (region.to_string(), label.to_string()))
        .collect()
}

fn open_browser(url: &str) {
    if try_open_browser(url).is_err() {
        console::warning("Unable to open a browser automatically. Please open the URL manually.");
    }
}

#[cfg(target_os = "macos")]
fn try_open_browser(url: &str) -> std::io::Result<()> {
    Command::new("open").arg(url).spawn().map(|_| ())
}

#[cfg(target_os = "windows")]
fn try_open_browser(url: &str) -> std::io::Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map(|_| ())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn try_open_browser(url: &str) -> std::io::Result<()> {
    Command::new("xdg-open").arg(url).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::json;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeDeviceAuthClient {
        device_code: Result<Auth0DeviceCode, String>,
        polls: RefCell<VecDeque<Result<Auth0TokenPoll, String>>>,
    }

    impl FakeDeviceAuthClient {
        fn new(polls: Vec<Result<Auth0TokenPoll, String>>) -> Self {
            Self {
                device_code: Ok(Auth0DeviceCode {
                    device_code: "device-code".to_string(),
                    user_code: "ABCD-EFGH".to_string(),
                    verification_uri_complete: "https://example.test/device".to_string(),
                    interval_seconds: 2,
                }),
                polls: RefCell::new(VecDeque::from(polls)),
            }
        }

        fn with_device_code_error(error: &str) -> Self {
            Self {
                device_code: Err(error.to_string()),
                polls: RefCell::new(VecDeque::new()),
            }
        }
    }

    impl DeviceAuthClient for FakeDeviceAuthClient {
        fn request_device_code(&self) -> Result<Auth0DeviceCode, String> {
            self.device_code.clone()
        }

        fn poll_device_token(&self, _device_code: &str) -> Result<Auth0TokenPoll, String> {
            self.polls
                .borrow_mut()
                .pop_front()
                .expect("test provided enough poll responses")
        }
    }

    #[test]
    fn sign_in_with_auth0_client_uses_httpmock_device_flow() {
        let server = MockServer::start();
        let auth0 = Auth0Client::with_base_url(server.base_url(), "client-id");
        let device_code = server.mock(|when, then| {
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
                "interval": 0,
            }));
        });
        let token = server.mock(|when, then| {
            when.method(POST)
                .path("/oauth/token")
                .json_body(json!({
                    "client_id": "client-id",
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                    "device_code": "device-code",
                }));
            then.status(200)
                .json_body(json!({ "access_token": "jwt-token" }));
        });
        let mut opened_url = None;
        let mut sleeps = Vec::new();

        let access_token = sign_in_with_client(
            &auth0,
            |url| opened_url = Some(url.to_string()),
            |duration| sleeps.push(duration),
        )
        .expect("successful sign in");

        assert_eq!(access_token, "jwt-token");
        assert_eq!(
            opened_url.as_deref(),
            Some("https://example.test/device")
        );
        assert_eq!(sleeps, vec![Duration::from_secs(1)]);
        device_code.assert_calls(1);
        token.assert_calls(1);
    }

    #[test]
    fn sign_in_with_client_polls_until_authorized_and_honors_slow_down() {
        let auth0 = FakeDeviceAuthClient::new(vec![
            Ok(Auth0TokenPoll::AuthorizationPending),
            Ok(Auth0TokenPoll::SlowDown),
            Ok(Auth0TokenPoll::Authorized {
                access_token: "jwt-token".to_string(),
            }),
        ]);
        let mut opened_url = None;
        let mut sleeps = Vec::new();

        let token = sign_in_with_client(
            &auth0,
            |url| opened_url = Some(url.to_string()),
            |duration| sleeps.push(duration),
        )
        .expect("successful sign in");

        assert_eq!(token, "jwt-token");
        assert_eq!(
            opened_url.as_deref(),
            Some("https://example.test/device")
        );
        assert_eq!(
            sleeps,
            vec![
                Duration::from_secs(2),
                Duration::from_secs(2),
                Duration::from_secs(7)
            ]
        );
    }

    #[test]
    fn sign_in_with_client_reports_expired_token() {
        let auth0 = FakeDeviceAuthClient::new(vec![Ok(Auth0TokenPoll::Expired)]);

        let error = sign_in_with_client(&auth0, |_| {}, |_| {}).expect_err("expired login");

        assert_eq!(
            error,
            "Authorization timed out. Please try again."
        );
    }

    #[test]
    fn sign_in_with_client_reports_device_code_errors() {
        let auth0 = FakeDeviceAuthClient::with_device_code_error("device-code failed");

        let error = sign_in_with_client(&auth0, |_| {}, |_| {}).expect_err("device-code error");

        assert_eq!(error, "device-code failed");
    }

    #[test]
    fn sign_in_reports_unknown_region_before_polling() {
        let error = sign_in("moon-1").expect_err("unknown region");

        assert!(error.contains("Unknown region: moon-1"));
    }
}
