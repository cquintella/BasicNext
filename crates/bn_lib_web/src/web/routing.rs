#![allow(clippy::wildcard_imports)]
use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Route {
    pub(crate) method: String,
    pub(crate) pattern: String,
    pub(crate) order: usize,
}

impl Route {
    pub(crate) fn pattern(&self) -> &str {
        &self.pattern
    }
}

pub(crate) fn select_route<'a>(
    routes: &'a [Route],
    method: &str,
    path: &str,
) -> Option<(&'a Route, HashMap<String, String>)> {
    routes
        .iter()
        .filter_map(|route| {
            if !valid_route_pattern(&route.pattern)
                || !valid_method(&route.method)
                || route.method != method
            {
                return None;
            }
            let mut parameters = HashMap::new();
            let pattern = route.pattern.split('/').collect::<Vec<_>>();
            let actual = path.split('/').collect::<Vec<_>>();
            if pattern.len() != actual.len() {
                return None;
            }
            let mut literals = 0;
            for (expected, found) in pattern.iter().zip(actual.iter()) {
                if let Some(name) = expected.strip_prefix(':') {
                    if found.is_empty() {
                        return None;
                    }
                    parameters.insert(name.to_string(), (*found).to_string());
                } else if expected != found {
                    return None;
                } else {
                    literals += 1;
                }
            }
            Some((route, literals, parameters))
        })
        .max_by_key(|(route, literals, _)| (*literals, usize::MAX - route.order))
        .map(|(route, _, parameters)| (route, parameters))
}

pub(crate) fn valid_route_pattern(pattern: &str) -> bool {
    if !pattern.starts_with('/')
        || pattern.contains('?')
        || pattern.contains('%')
        || pattern.contains('\\')
        || pattern.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return false;
    }
    let mut names = std::collections::HashSet::new();
    pattern.split('/').all(|segment| {
        if let Some(name) = segment.strip_prefix(':') {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                && names.insert(name)
        } else {
            !segment.contains(':')
        }
    })
}

pub(crate) fn valid_method(method: &str) -> bool {
    !method.is_empty()
        && method.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

pub(crate) fn allowed_methods(routes: &[Route], path: &str) -> Vec<String> {
    let mut methods = routes
        .iter()
        .filter_map(|route| {
            if !valid_route_pattern(&route.pattern) || !valid_method(&route.method) {
                return None;
            }
            let pattern = route.pattern.split('/').collect::<Vec<_>>();
            let actual = path.split('/').collect::<Vec<_>>();
            if pattern.len() != actual.len()
                || pattern
                    .iter()
                    .zip(&actual)
                    .any(|(expected, found)| !expected.starts_with(':') && expected != found)
            {
                return None;
            }
            Some(route.method.clone())
        })
        .collect::<Vec<_>>();
    methods.sort();
    methods.dedup();
    methods
}

pub(crate) fn route_for_request<'a>(
    routes: &'a [Route],
    method: &str,
    path: &str,
) -> Option<(&'a Route, HashMap<String, String>)> {
    select_route(routes, method, path).or_else(|| {
        (method == "HEAD")
            .then(|| select_route(routes, "GET", path))
            .flatten()
    })
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RouteOutcome<'a> {
    Matched(&'a Route, HashMap<String, String>),
    NotFound,
    MethodNotAllowed(Vec<String>),
}

pub(crate) fn dispatch_route<'a>(
    routes: &'a [Route],
    method: &str,
    path: &str,
) -> RouteOutcome<'a> {
    if let Some((route, parameters)) = route_for_request(routes, method, path) {
        return RouteOutcome::Matched(route, parameters);
    }
    let allowed = allowed_methods(routes, path);
    if allowed.is_empty() {
        RouteOutcome::NotFound
    } else {
        RouteOutcome::MethodNotAllowed(allowed)
    }
}
