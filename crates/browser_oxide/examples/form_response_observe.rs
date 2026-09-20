//! Observe an existing page's named form response without reading its value.
//!
//! No widgets, input events, form submissions, or callback wrappers are injected.
//! A non-empty field is not proof of a particular callback or server validation.
//! Run with a clean environment (no diagnostic dumps or persistent cookie jar).

use std::{process::ExitCode, time::Duration};

const USAGE: &str =
    "Usage: form_response_observe <http(s)-url> <control-name> [timeout-seconds:1..300]";

struct Config {
    url: String,
    name: String,
    timeout: Duration,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, &'static str> {
        if !(2..=3).contains(&args.len()) || args[1].is_empty() {
            return Err(USAGE);
        }
        let url = url::Url::parse(&args[0]).map_err(|_| "invalid URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("expected HTTP(S) URL without credentials");
        }
        let seconds = args
            .get(2)
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| "invalid timeout")?
            .unwrap_or(120);
        if !(1..=300).contains(&seconds) {
            return Err("timeout must be between 1 and 300 seconds");
        }
        Ok(Self {
            url: url.into(),
            name: args[1].clone(),
            timeout: Duration::from_secs(seconds),
        })
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let config = match Config::parse(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    // Engine diagnostics can persist page data or alter the observed flow.
    // Reject inherited overrides instead of silently weakening this contract.
    if std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("BROWSER_OXIDE_")) {
        eprintln!("unset BROWSER_OXIDE_* overrides before running this read-only observer");
        return ExitCode::from(2);
    }
    browser_oxide::js_runtime::block_on_v8_thread("form-response-observe", move || async move {
        match tokio::time::timeout(config.timeout, observe(&config)).await {
            Ok(Ok(())) => ExitCode::SUCCESS,
            Ok(Err(stage)) => {
                eprintln!("RESPONSE_FOUND=false reason={stage}");
                ExitCode::FAILURE
            }
            Err(_) => {
                eprintln!("RESPONSE_FOUND=false reason=deadline");
                ExitCode::FAILURE
            }
        }
    })
}

async fn observe(config: &Config) -> Result<(), &'static str> {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let client = browser_oxide::net::HttpClient::shared(&profile).map_err(|_| "http-client")?;
    let mut page = browser_oxide::Page::navigate_pure(&config.url, profile.clone(), 2)
        .await
        .map_err(|_| "navigation")?;
    let mut previous = None;
    loop {
        let lengths = page
            .named_form_control_value_lengths(&config.name)
            .map_err(|_| "observe-controls")?;
        let state = (lengths.clone(), page.frame_tree_count());
        if previous.as_ref() != Some(&state) {
            println!("OBSERVE lengths={lengths:?} frames={}", state.1);
            previous = Some(state);
        }
        if let Some(length) = lengths.into_iter().find(|length| *length > 0) {
            println!("RESPONSE_FOUND=true len={length} callback_observed=unknown");
            return Ok(());
        }
        page.drive_frame_tree(&client, &profile).await;
        page.event_loop()
            .run_until_settled(Duration::from_millis(200))
            .await
            .map_err(|_| "event-loop")?;
        // Settled is not a business callback deadline and can return early.
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn config_preserves_arbitrary_names_and_bounds_deadline() {
        let config =
            Config::parse(&args(&["https://example.test/", "a\"] [name=\"b", "3"])).unwrap();
        assert_eq!(config.name, "a\"] [name=\"b");
        assert_eq!(config.timeout, Duration::from_secs(3));
        for timeout in ["0", "301", "-1", "invalid"] {
            assert!(Config::parse(&args(&["https://example.test/", "response", timeout])).is_err());
        }
    }

    #[test]
    fn config_rejects_credentials_non_web_urls_and_missing_names() {
        for url in [
            "https://user:password@example.test/",
            "file:///tmp/a",
            "not a URL",
        ] {
            assert!(Config::parse(&args(&[url, "response"])).is_err());
        }
        assert!(Config::parse(&args(&["https://example.test/", ""])).is_err());
        assert!(Config::parse(&[]).is_err());
    }
}
