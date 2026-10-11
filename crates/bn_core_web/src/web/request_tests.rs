// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Route parameters, request cookies and response cookies (bucket 0.6.5
//! E.S0.d).

use super::Route;
use super::route_for_request;
use std::net::IpAddr;

/// `Request.Param` reads the matched `:name` segments; `Request.Cookie`
/// reads every `Cookie` header (RFC 6265 §5.4), unquotes a quoted value and
/// keeps the first pair with the name (bucket 0.6.5 E.S0.d).
#[test]
fn request_params_and_cookies_follow_the_route_and_rfc_6265() {
    let peer: IpAddr = "127.0.0.1".parse().unwrap();
    let mut request = super::Request::new(
        "GET",
        "/users/00102?id=7",
        vec![
            ("cookie".into(), "theme=dark; sid=\"abc\"".into()),
            ("Cookie".into(), "sid=second; lang=pt".into()),
        ],
        "",
        peer,
    )
    .unwrap();
    let routes = [Route {
        method: "GET".into(),
        pattern: "/users/:id".into(),
        order: 0,
    }];
    let (_, found) = route_for_request(&routes, "GET", "/users/00102").expect("the route matches");
    request.params = super::sorted_params(&found);
    assert_eq!(request.param("id"), Some("00102"));
    assert_eq!(request.param("missing"), None);
    assert_eq!(request.cookie("sid"), Some("abc"));
    assert_eq!(request.cookie("lang"), Some("pt"));
    assert_eq!(request.cookie("theme"), Some("dark"));
    assert_eq!(request.cookie("absent"), None);
    assert_eq!(request.query("id").unwrap().get(0), Some("7"));
}

/// One cookie policy rule for jars and responses: `SameSite` names, `None`
/// needs `Secure`, a negative age is refused; a cleartext response clears a
/// cookie without `Secure` and refuses to set a `Secure` one.
#[test]
fn cookie_policy_and_response_cookies() {
    use crate::web_state::{CookieOptions, SameSite, cookie_max_age};
    assert_eq!(
        CookieOptions::from_policy(false, true, "Strict")
            .unwrap()
            .same_site,
        SameSite::Strict
    );
    assert_eq!(
        CookieOptions::from_policy(true, true, "Sometimes"),
        Err("invalid SameSite policy")
    );
    assert_eq!(
        CookieOptions::from_policy(false, true, "None"),
        Err("SameSite=None requires Secure")
    );
    assert_eq!(cookie_max_age(-1), Err("negative cookie age"));
    assert_eq!(cookie_max_age(1500).unwrap().as_millis(), 1500);

    let mut cleartext = super::Response::new();
    assert_eq!(
        cleartext.set_cookie("sid", "v", None, "/", None, CookieOptions::default()),
        Err("Secure cookies cannot be sent over cleartext HTTP")
    );
    cleartext.clear_cookie("sid", None, "/").unwrap();
    assert_eq!(
        cleartext.headers.last().unwrap().1,
        "sid=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
    );
    let mut tls = super::Response::new();
    tls.secure = true;
    tls.clear_cookie("sid", None, "/").unwrap();
    assert!(tls.headers.last().unwrap().1.contains("; Secure"));
}
