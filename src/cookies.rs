use axum_extra::extract::cookie::{Cookie, Expiration, SameSite};
use std::time::Duration;
use tower_cookies::{Cookies, PrivateCookies, cookie::time::OffsetDateTime};

pub enum CookieStatus {
    Added,
    Unchanged,
}

// Return true if a cookie exists with the given value already set
pub fn cookie_exists(cookies: &Cookies, name: impl Into<String>, value: impl Into<String>) -> bool {
    let name = name.into();
    let value = value.into();
    match cookies.get(name.as_str()) {
        Some(cookie) => cookie.value() == value,
        None => false,
    }
}

// Create a cookie for returning to the caller in a way that can be consumed by Javascript in the
// browser
pub fn add_browser_cookie(
    cookies: &Cookies,
    name: impl Into<String>,
    value: impl Into<String>,
) -> CookieStatus {
    let name = name.into();
    let value = value.into();

    if cookie_exists(cookies, name.clone(), value.clone()) {
        return CookieStatus::Unchanged;
    }

    cookies.add(browser_cookie(name, value));
    CookieStatus::Added
}

fn browser_cookie<'a>(name: String, value: String) -> Cookie<'a> {
    Cookie::build((name, value))
        .same_site(SameSite::Strict)
        .secure(true)
        .path("/")
        .build()
}

fn server_cookie<'a>(name: String, value: String, expiry: Option<Duration>) -> Cookie<'a> {
    let mut cookie = Cookie::build((name, value))
        .same_site(SameSite::Strict)
        .secure(true)
        .path("/")
        .build();

    if let Some(expiry) = expiry {
        cookie.set_expires(Expiration::from(OffsetDateTime::now_utc() + expiry));
    }
    cookie
}

// Return true if a cookie exists with the given value already set
fn private_cookie_exists(
    cookies: &PrivateCookies,
    name: impl Into<String>,
    value: impl Into<String>,
) -> bool {
    let name = name.into();
    let value = value.into();
    match cookies.get(name.as_str()) {
        Some(cookie) => cookie.value() == value,
        None => false,
    }
}

// Create a cookie that is only accessible by the server, and not consumable by the Javascript
pub fn add_private_server_cookie(
    cookies: &PrivateCookies,
    name: impl Into<String>,
    value: impl Into<String>,
    expiry: Option<Duration>,
) -> CookieStatus {
    let name = name.into();
    let value = value.into();
    if private_cookie_exists(cookies, name.clone(), value.clone()) {
        return CookieStatus::Unchanged;
    }

    cookies.add(server_cookie(name, value, expiry));
    CookieStatus::Added
}
