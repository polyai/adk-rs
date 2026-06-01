use crate::{LoginArgs, console, credentials, init::INIT_REGIONS, prompt_select};
use adk_api_client::{Auth0Client, Auth0TokenPoll, HttpPlatformClient, JupiterClient};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

const LOGIN_REGIONS: &[&str] = INIT_REGIONS;

pub(crate) fn cmd_login(args: LoginArgs) -> ExitCode {
    let region = match resolve_login_region(args.region) {
        Ok(Some(region)) => region,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            crate::emit_error(false, &error);
            return ExitCode::from(1);
        }
    };
    print_welcome_message();
    match sign_in_and_save_key(&region) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            crate::emit_error(false, &error);
            ExitCode::from(1)
        }
    }
}

pub(crate) fn print_welcome_message() {
    console::plain("[label]Welcome to the PolyAI ADK![/label]");
}

pub(crate) fn sign_in_and_save_key(region: &str) -> Result<String, String> {
    let jwt_access_token = sign_in(region)?;
    let api_key = authenticate_and_save_key(region, &jwt_access_token)?;
    wait_for_api_key_active(region, &api_key)?;
    console::success("Authentication complete.");
    Ok(api_key)
}

pub(crate) fn sign_in(region: &str) -> Result<String, String> {
    let auth0 = Auth0Client::new(region).map_err(|error| error.to_string())?;
    let device_code = auth0
        .request_device_code()
        .map_err(|error| format!("Failed to request device code: {error}"))?;

    console::info(format!(
        "Open the following URL in your browser and enter code {}:",
        device_code.user_code
    ));
    console::plain(format!("[label]{}[/label]", device_code.verification_uri_complete));
    open_browser(&device_code.verification_uri_complete);

    let mut interval = device_code.interval_seconds.max(1);
    loop {
        thread::sleep(Duration::from_secs(interval));
        match auth0
            .poll_device_token(&device_code.device_code)
            .map_err(|error| format!("Failed to poll for authorization: {error}"))?
        {
            Auth0TokenPoll::Authorized { access_token } => return Ok(access_token),
            Auth0TokenPoll::AuthorizationPending => {}
            Auth0TokenPoll::SlowDown => interval += 5,
            Auth0TokenPoll::Expired => {
                return Err("Login request expired. Please run the command again.".to_string());
            }
        }
    }
}

pub(crate) fn authenticate_and_save_key(
    region: &str,
    jwt_access_token: &str,
) -> Result<String, String> {
    let jupiter = JupiterClient::new(region).map_err(|error| error.to_string())?;
    jupiter
        .authorise(jwt_access_token)
        .map_err(|error| format!("Failed to authorize account: {error}"))?;

    let api_key = match jupiter
        .list_personal_access_tokens(jwt_access_token)
        .map_err(|error| format!("Failed to list API keys: {error}"))?
        .into_iter()
        .next()
    {
        Some(token) => {
            console::info(format!(
                "Using existing API key {}.",
                credentials::mask_api_key(&token.key)
            ));
            token.key
        }
        None => {
            let key = jupiter
                .create_personal_access_token(jwt_access_token, "adk-key")
                .map_err(|error| format!("Failed to create API key: {error}"))?;
            console::info(format!(
                "Created API key {}.",
                credentials::mask_api_key(&key)
            ));
            key
        }
    };

    let path = credentials::save_api_key_credential_file(&api_key, region)?;
    console::success(format!("Saved API key to {}.", path.display()));
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
            let choices = LOGIN_REGIONS
                .iter()
                .map(|region| ((*region).to_string(), (*region).to_string()))
                .collect::<Vec<_>>();
            let selected = prompt_select("Select Region", &choices)?;
            if selected.is_none() {
                console::warning("No region selected. Exiting.");
            }
            Ok(selected)
        }
    }
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
