//! Browser capabilities are selected by native application identity, never command phrasing.
use crate::Result;

pub const KINDS: &[&str] = &[
    "browser_new_tab",
    "browser_close_tab",
    "browser_next_tab",
    "browser_previous_tab",
    "browser_back",
    "browser_forward",
    "browser_reload",
    "browser_navigate",
];

pub fn supported(platform: &str, identity: &str) -> bool {
    match platform {
        "macos" => matches!(
            identity,
            "com.apple.Safari"
                | "com.google.Chrome"
                | "org.chromium.Chromium"
                | "org.mozilla.firefox"
                | "com.microsoft.edgemac"
                | "com.brave.Browser"
        ),
        "windows" => identity.strip_prefix("win32:").is_some_and(|path| {
            // The native adapter supplies a full process image path, not a window title.
            path.rsplit_once(['\\', '/']).is_some_and(|(_, name)| {
                [
                    "chrome.exe",
                    "chromium.exe",
                    "firefox.exe",
                    "msedge.exe",
                    "brave.exe",
                ]
                .iter()
                .any(|expected| name.eq_ignore_ascii_case(expected))
            })
        }),
        _ => false,
    }
}

/// Require browser chrome all the way to its window: a webpage can expose its own toolbar.
pub fn address_in_toolbar<'a>(ancestors: impl IntoIterator<Item = &'a str>) -> bool {
    let mut toolbar = false;
    for role in ancestors {
        match role {
            "AXToolbar" => toolbar = true,
            "AXWebArea" | "AXSheet" | "AXDialog" => return false,
            "AXWindow" => return toolbar,
            _ => {}
        }
    }
    false
}

/// Accept explicit web addresses only. A bare domain gets HTTPS, never a search query.
pub fn navigation_url(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty()
        || text.len() > 2048
        || text.chars().any(|c| c.is_whitespace() || c.is_control())
        || text.starts_with('/')
        || text.contains('\\')
    {
        return Err("Provide a web URL or domain, such as https://example.com.".into());
    }
    let explicit = text.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    let input = if explicit {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let url = url::Url::parse(&input).map_err(|_| "The web URL is invalid.")?;
    let host = url.host_str().ok_or("The web URL has no host.")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || (!explicit && !host.contains('.') && host != "localhost" && !host.starts_with('['))
    {
        return Err("Provide an HTTP or HTTPS URL without embedded credentials.".into());
    }
    Ok(url.into())
}
