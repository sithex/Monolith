//! Navigation guard for the island webview.
//!
//! WebKitGTK loads a file or link that is dropped on a page which does not handle the drop
//! itself, replacing the island page with that file. The page blocks such drops
//! (`ui/island/drop-guard.ts`); this is the second layer: the webview may only ever navigate
//! within the app's own origin (`tauri://localhost`, or the dev server in a dev build).
//! Tauri allows every navigation by default.
use tauri::Url;

/// The origin the app's pages are served from in this build.
pub fn app_origin(dev: bool, dev_url: Option<&Url>) -> Option<Url> {
    match dev_url.filter(|_| dev) {
        Some(url) => Some(url.clone()),
        None => Url::parse("tauri://localhost").ok(),
    }
}

/// True only for a page on exactly the app's own scheme, host and port. Compared by hand:
/// `tauri:` is not a special scheme, so `Url::origin` is opaque and never equal.
pub fn allowed(target: &Url, app: &Url) -> bool {
    target.scheme() == app.scheme()
        && target.host_str().is_some()
        && target.host_str() == app.host_str()
        && target.port_or_known_default() == app.port_or_known_default()
        && target.username().is_empty()
        && target.password().is_none()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn only_the_apps_own_origin_may_load() {
        let app = app_origin(false, None).unwrap();
        for ok in [
            "tauri://localhost",
            "tauri://localhost/island.html",
            "tauri://localhost/legacy.html?x=1#top",
            "tauri://localhost/spike.html",
        ] {
            assert!(allowed(&url(ok), &app), "{ok} must stay allowed");
        }
        for blocked in [
            "file:///data/user/Downloads/a.txt",
            "file://localhost/etc/passwd",
            "http://localhost/",
            "https://example.com/",
            "tauri://evil/island.html",
            "tauri://localhost.example.com/",
            "tauri://user@localhost/",
            "about:blank",
            "data:text/html,hi",
            "javascript:alert(1)",
        ] {
            assert!(!allowed(&url(blocked), &app), "{blocked} must be blocked");
        }
    }

    #[test]
    fn a_dev_build_allows_only_its_dev_server() {
        let dev = url("http://localhost:5173");
        let app = app_origin(true, Some(&dev)).unwrap();
        assert!(allowed(&url("http://localhost:5173/island.html"), &app));
        assert!(!allowed(&url("http://localhost:5174/island.html"), &app));
        assert!(!allowed(&url("tauri://localhost/island.html"), &app));
        assert!(!allowed(&url("file:///tmp/x"), &app));
        // A release build ignores a configured dev URL.
        assert_eq!(app_origin(false, Some(&dev)).unwrap().as_str(), "tauri://localhost");
    }
}
