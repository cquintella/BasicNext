// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::Value;

pub(super) fn render(value: &Value) -> String {
    match value {
        Value::Integer(value, _) => value.to_string(),
        Value::Float(value, _) if value.is_nan() => "NAN".into(),
        Value::Float(value, _) if *value == f64::INFINITY => "INF".into(),
        Value::Float(value, _) if *value == f64::NEG_INFINITY => "-INF".into(),
        Value::Float(value, _) => {
            let mut text = value.to_string();
            if !text.contains(['.', 'e', 'E']) {
                text.push_str(".0");
            }
            text
        }
        Value::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        Value::String(value) => value.clone(),
        Value::Null => "NULL".into(),
        Value::NotAvailable => "NA".into(),
        Value::EndOfFile => "EOF".into(),
        Value::Vector(values) => format!(
            "[{}]",
            values.iter().map(render).collect::<Vec<_>>().join(", ")
        ),
        Value::Function(name) | Value::Type(name) | Value::TimeZone(name) => name.clone(),
        Value::HostConsole => "HOST.Console".into(),
        Value::HostArgs => "HOST.Args".into(),
        Value::TcpStream(_) => "HOST.Net.TCPStream".into(),
        Value::TcpListener(_) => "HOST.Net.TCPListener".into(),
        Value::UdpSocket(_) => "HOST.Net.UDPSocket".into(),
        Value::LogFields(_) => "BNLog.Fields".into(),
        Value::LogEntry(_) => "BNLog.Entry".into(),
        Value::LogLogger(_) => "BNLog.Logger".into(),
        Value::Json(_) => "BNJson.Json".into(),
        Value::DispatchQueue(id) => {
            debug_assert_ne!(*id, 0);
            "BNDispatch.Queue".into()
        }
        Value::DispatchTicket(id) => format!("BNDispatch.Ticket#{id}"),
        Value::DispatchGroup(id) => format!("BNDispatch.Group#{id}"),
        Value::DispatchBarrier(id) => format!("BNDispatch.Barrier#{id}"),
        Value::DispatchSemaphore(id) => format!("BNDispatch.Semaphore#{id}"),
        Value::DispatchMutex(id) => format!("BNDispatch.Mutex#{id}"),
        Value::Handle { type_name } | Value::Record { type_name, .. } => type_name.clone(),
        Value::Object { class, .. } => class.rsplit('.').next().unwrap_or(class).to_string(),
        Value::Pointer { .. } => "POINTER".into(),
        Value::File(_) => "FS.File".into(),
        Value::DataFrame(_) => "DataFrame".into(),
        Value::Date(days) => crate::temporal::format_date(*days),
        Value::Time(millis) => crate::temporal::format_time(*millis),
        Value::Error { code, message } => format!("Error({code}, {message})"),
    }
}
