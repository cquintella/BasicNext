#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn web_state_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        if name.contains(".SessionStore.") {
            if method == "New" {
                require_arity(name, arguments, 2, span)?;
                let capacity = integer(&arguments[0], span)?.0;
                let idle = integer(&arguments[1], span)?.0;
                if idle < 1 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid session idle timeout".into(),
                    });
                }
                let store = crate::web_state::SessionStore::new(
                    capacity,
                    std::time::Duration::from_millis(u64::try_from(idle).unwrap_or(0)),
                )
                .map_err(|message| runtime_error("SESSION_CONFIG", message, span))?;
                let object = self.allocate_object("BNWeb.SessionStore", span)?;
                if let Value::Object { handle, .. } = object {
                    self.web_session_stores.insert(handle, store);
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "SessionStore receiver must be an object",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 3, span)?;
                let capacity = integer(&arguments[1], span)?.0;
                let idle = integer(&arguments[2], span)?.0;
                if idle < 1 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid session idle timeout".into(),
                    });
                }
                let store = crate::web_state::SessionStore::new(
                    capacity,
                    std::time::Duration::from_millis(u64::try_from(idle).unwrap_or(0)),
                )
                .map_err(|message| runtime_error("SESSION_CONFIG", message, span))?;
                self.web_session_stores.insert(*handle, store);
                return Ok(Value::Null);
            }
            let store = self.web_session_stores.get_mut(handle).ok_or_else(|| {
                runtime_error("STALE_HANDLE", "SessionStore handle is not live", span)
            })?;
            match method {
                "Create" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(value) = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "session value must be STRING",
                            span,
                        ));
                    };
                    Ok(store
                        .create(value)
                        .map_or_else(|message| Value::Error { code: 1, message }, Value::String))
                }
                "Get" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(id) = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "session id must be STRING",
                            span,
                        ));
                    };
                    Ok(store.get(id).map_or(
                        Value::Error {
                            code: 1,
                            message: "session not found".into(),
                        },
                        Value::String,
                    ))
                }
                "Delete" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::String(id) = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "session id must be STRING",
                            span,
                        ));
                    };
                    Ok(store.delete(id).map_or_else(
                        |message| Value::Error { code: 1, message },
                        |()| Value::Null,
                    ))
                }
                "Set" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(id), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "session id/value must be STRING",
                            span,
                        ));
                    };
                    Ok(store
                        .set(id, value)
                        .map_or_else(|message| Value::Error { code: 1, message }, |()| Value::Null))
                }
                "Rotate" => {
                    require_arity(name, arguments, 3, span)?;
                    let (Value::String(id), Value::String(value)) = (&arguments[1], &arguments[2])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "session id/value must be STRING",
                            span,
                        ));
                    };
                    Ok(store
                        .rotate(id, value)
                        .map_or_else(|message| Value::Error { code: 1, message }, Value::String))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "SessionStore provider unavailable".into(),
                }),
            }
        } else if name.contains(".Scraper.") {
            if method == "Parse" {
                require_arity(name, arguments, 1, span)?;
                let Value::String(html) = &arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "HTML must be STRING", span));
                };
                let scraper = match crate::web_state::Scraper::parse(html) {
                    Ok(value) => value,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                let object = self.allocate_object("BNWeb.Scraper", span)?;
                if let Value::Object { handle, .. } = object {
                    self.web_scrapers.insert(handle, scraper);
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "Scraper receiver must be an object",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 2, span)?;
                let Value::String(html) = &arguments[1] else {
                    return Err(runtime_error("TYPE_MISMATCH", "HTML must be STRING", span));
                };
                let scraper = crate::web_state::Scraper::parse(html)
                    .map_err(|message| runtime_error("SCRAPER_INPUT", message, span))?;
                self.web_scrapers.insert(*handle, scraper);
                return Ok(Value::Null);
            }
            let scraper = self
                .web_scrapers
                .get(handle)
                .ok_or_else(|| runtime_error("STALE_HANDLE", "Scraper handle is not live", span))?;
            if method == "Text" {
                require_arity(name, arguments, 2, span)?;
                let Value::String(selector) = &arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "selector must be STRING",
                        span,
                    ));
                };
                return Ok(scraper
                    .text(selector)
                    .map_or_else(|message| Value::Error { code: 1, message }, Value::String));
            }
            Ok(Value::Error {
                code: 1,
                message: "Scraper provider unavailable".into(),
            })
        } else if name.contains(".ACL.") {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "ACL receiver must be an object",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                self.web_acls.insert(*handle, crate::web_state::Acl::new());
                return Ok(Value::Null);
            }
            let acl = self
                .web_acls
                .get_mut(handle)
                .ok_or_else(|| runtime_error("STALE_HANDLE", "ACL handle is not live", span))?;
            match method {
                "Allow" | "Deny" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Record { fields, .. } = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "CIDR must be HOST.Net.CIDR",
                            span,
                        ));
                    };
                    let (Value::String(network), Value::Integer(prefix, _)) = (
                        fields
                            .get("network")
                            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid CIDR", span))?,
                        fields
                            .get("prefix")
                            .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid CIDR", span))?,
                    ) else {
                        return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR", span));
                    };
                    let cidr = format!("{network}/{prefix}");
                    let result = if method == "Allow" {
                        acl.allow(&cidr)
                    } else {
                        acl.deny(&cidr)
                    };
                    Ok(result.map_or_else(
                        |message| Value::Error { code: 1, message },
                        |()| Value::Null,
                    ))
                }
                "Check" => {
                    require_arity(name, arguments, 2, span)?;
                    let Value::Record { fields, .. } = &arguments[1] else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "ACL.Check expects a HOST.Net address",
                            span,
                        ));
                    };
                    let Value::String(text) = fields.get("value").ok_or_else(|| {
                        runtime_error("TYPE_MISMATCH", "invalid address record", span)
                    })?
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "invalid address record",
                            span,
                        ));
                    };
                    let address = text
                        .parse()
                        .map_err(|_| runtime_error("TYPE_MISMATCH", "invalid address", span))?;
                    Ok(Value::Boolean(acl.check(address)))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "ACL provider unavailable".into(),
                }),
            }
        } else if name.contains(".CookieJar.") {
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "CookieJar receiver must be an object",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                self.web_cookie_jars
                    .insert(*handle, crate::web_state::CookieJar::new());
                return Ok(Value::Null);
            }
            let jar = self.web_cookie_jars.get_mut(handle).ok_or_else(|| {
                runtime_error("STALE_HANDLE", "CookieJar handle is not live", span)
            })?;
            match method {
                "Set" => {
                    require_arity(name, arguments, 6, span)?;
                    let (Value::String(n), Value::String(v), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3], &arguments[4])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "cookie arguments must be STRING",
                            span,
                        ));
                    };
                    let age = integer(&arguments[5], span)?.0;
                    if age < 0 {
                        return Ok(Value::Error {
                            code: 1,
                            message: "negative cookie age".into(),
                        });
                    }
                    Ok(jar
                        .set(
                            n,
                            v,
                            d,
                            p,
                            std::time::Duration::from_millis(u64::try_from(age).unwrap_or(0)),
                        )
                        .map_or_else(
                            |message| Value::Error { code: 1, message },
                            |()| Value::Null,
                        ))
                }
                "Get" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(n), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "cookie lookup expects STRING",
                            span,
                        ));
                    };
                    Ok(jar.get(n, d, p).map_or(
                        Value::Error {
                            code: 1,
                            message: "cookie not found".into(),
                        },
                        Value::String,
                    ))
                }
                "Delete" => {
                    require_arity(name, arguments, 4, span)?;
                    let (Value::String(n), Value::String(d), Value::String(p)) =
                        (&arguments[1], &arguments[2], &arguments[3])
                    else {
                        return Err(runtime_error(
                            "TYPE_MISMATCH",
                            "cookie delete expects STRING",
                            span,
                        ));
                    };
                    jar.delete(n, d, p);
                    Ok(Value::Null)
                }
                "Count" => Ok(Value::Integer(jar.len() as i128, IntegerType::Int32)),
                _ => Ok(Value::Error {
                    code: 1,
                    message: "CookieJar provider unavailable".into(),
                }),
            }
        } else if name.contains(".TLSConfig.") {
            if method == "FromPEM" {
                require_arity(name, arguments, 2, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[0], &arguments[1])
                else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "TLS material must be STRING",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                let object = self.allocate_object("BNWeb.TLSConfig", span)?;
                if let Value::Object { handle, .. } = object {
                    self.web_tls_configs
                        .insert(handle, std::sync::Arc::new(config));
                }
                return Ok(object);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "TLSConfig receiver must be an object",
                    span,
                ));
            };
            if method == "CONSTRUCTOR" {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[1], &arguments[2])
                else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "TLS material must be STRING",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                self.web_tls_configs
                    .insert(*handle, std::sync::Arc::new(config));
                return Ok(Value::Null);
            }
            if method == "FromPEM" {
                require_arity(name, arguments, 3, span)?;
                let (Value::String(cert), Value::String(key)) = (&arguments[1], &arguments[2])
                else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "TLS material must be STRING",
                        span,
                    ));
                };
                let config = match crate::tls::server_config_from_pem(cert, key) {
                    Ok(config) => config,
                    Err(message) => return Ok(Value::Error { code: 1, message }),
                };
                self.web_tls_configs
                    .insert(*handle, std::sync::Arc::new(config));
                return Ok(Value::Object {
                    handle: *handle,
                    class: "BNWeb.TLSConfig".into(),
                });
            }
            Ok(Value::Error {
                code: 1,
                message: "BNWeb.TLSConfig provider unavailable".into(),
            })
        } else if name.contains(".HeaderValues.") || name.contains(".QueryValues.") {
            if method == "CONSTRUCTOR" {
                let Some(Value::Object { handle, .. }) = arguments.first() else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "BNWeb values receiver must be an object",
                        span,
                    ));
                };
                self.web_values.insert(*handle, Vec::new());
                return Ok(Value::Null);
            }
            let Some(Value::Object { handle, .. }) = arguments.first() else {
                return Err(runtime_error(
                    "TYPE_MISMATCH",
                    "BNWeb values receiver must be an object",
                    span,
                ));
            };
            let values = self.web_values.get(handle).ok_or_else(|| {
                runtime_error("STALE_HANDLE", "BNWeb values handle is not live", span)
            })?;
            match method {
                "Count" => {
                    require_arity(name, arguments, 1, span)?;
                    integer_from_count(values.len(), span)
                }
                "Get" => {
                    require_arity(name, arguments, 2, span)?;
                    let index = integer(&arguments[1], span)?.0;
                    let Ok(index) = usize::try_from(index) else {
                        return Ok(Value::Error {
                            code: 1,
                            message: "index is outside collection".into(),
                        });
                    };
                    Ok(values.get(index).map_or_else(
                        || Value::Error {
                            code: 1,
                            message: "index is outside collection".into(),
                        },
                        |value| Value::String(value.clone()),
                    ))
                }
                _ => Ok(Value::Error {
                    code: 1,
                    message: "BNWeb provider unavailable".into(),
                }),
            }

        }
        else {
            Err(runtime_error("HOST_CAPABILITY_UNAVAILABLE", format!("web function '{name}' is not available"), span))
        }
    }
}
