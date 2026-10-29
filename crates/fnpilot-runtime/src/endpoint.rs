use fnpilot_core::Result;
use reqwest::Url;

pub fn local_url(endpoint: &str, model: &str, path: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint).map_err(|_| "Choose an HTTP loopback endpoint.")?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || model.trim().is_empty()
        || model.to_lowercase().contains("cloud")
    {
        return Err("Choose a downloaded local model and an HTTP loopback endpoint. Cloud inference is disabled.".into());
    }
    // Pin localhost to loopback rather than trusting system DNS or hosts-file overrides.
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| "Invalid host.")?;
    }
    url.set_path(&format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        path.trim_start_matches('/')
    ));
    Ok(url)
}
