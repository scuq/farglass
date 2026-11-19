//! The D-Bus bridge of the agent: method calls and signals for the shell, as the user.
//!
//! The agent permits only the bus names of its allowlist: as the destination of
//! a call, and as the sender of a subscription. A call to a system service that
//! a polkit policy protects can ask for an interactive authentication. The
//! polkit agent of the session (F019) then shows the prompt in the shell.

use std::collections::HashMap;
use std::sync::Arc;

use farglass_proto::message::{Bus, DbusEvent};
use futures_util::StreamExt as _;
use serde_json::{Map, Number, Value as Json};
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;
use zbus::message::{Flags, Type as MessageType};
use zbus::wire::StructureBuilder;
use zbus::{Array, Dict, ObjectPath, Signature, Str, Structure, Variant as Value};
use zbus::{Connection, MatchRule, MessageStream};

/// The error name of a request that the allowlist refuses.
pub const ACCESS_DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

/// The error name of arguments that do not fit their signature.
pub const INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";

/// The error name of a bus that is not available.
pub const NO_BUS: &str = "org.freedesktop.DBus.Error.NoServer";

/// The bus names that the shell can use.
///
/// An entry is a bus name, or a prefix that ends with `.*`, for example `org.freedesktop.portal.*`.
#[derive(Clone, Debug)]
pub struct DbusOptions {
    /// The names on the system bus.
    pub system: Vec<String>,
    /// The names on the session bus.
    pub session: Vec<String>,
}

impl Default for DbusOptions {
    /// The services that the settings of the shell use.
    fn default() -> Self {
        let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect();
        Self {
            system: names(&[
                "org.freedesktop.DBus",
                "org.freedesktop.hostname1",
                "org.freedesktop.locale1",
                "org.freedesktop.login1",
                "org.freedesktop.NetworkManager",
                "org.freedesktop.timedate1",
                "org.freedesktop.UPower",
            ]),
            session: names(&["org.freedesktop.DBus"]),
        }
    }
}

impl DbusOptions {
    /// Reports whether the shell can use the bus name `name` on `bus`.
    #[must_use]
    pub fn allows(&self, bus: Bus, name: &str) -> bool {
        let list = match bus {
            Bus::System => &self.system,
            Bus::Session => &self.session,
        };
        list.iter().any(|entry| match entry.strip_suffix(".*") {
            Some(prefix) => name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.')),
            None => entry == name,
        })
    }
}

/// The connections of the agent to the two buses. Each one opens at its first use.
#[derive(Debug)]
pub struct Buses {
    options: DbusOptions,
    system: OnceCell<Connection>,
    session: OnceCell<Connection>,
}

impl Buses {
    /// Returns the buses with the allowlist `options`.
    #[must_use]
    pub fn new(options: DbusOptions) -> Arc<Self> {
        Arc::new(Self {
            options,
            system: OnceCell::new(),
            session: OnceCell::new(),
        })
    }

    /// Returns the allowlist.
    #[must_use]
    pub const fn options(&self) -> &DbusOptions {
        &self.options
    }

    /// Returns the connection to `bus`, and opens it the first time.
    ///
    /// # Errors
    ///
    /// A zbus error if the bus is not available.
    pub async fn connection(&self, bus: Bus) -> zbus::Result<Connection> {
        let cell = match bus {
            Bus::System => &self.system,
            Bus::Session => &self.session,
        };
        cell.get_or_try_init(|| async {
            match bus {
                Bus::System => Connection::system().await,
                Bus::Session => Connection::session().await,
            }
        })
        .await
        .cloned()
    }
}

/// A method call of the shell.
#[derive(Clone, Debug)]
pub struct Call {
    /// The bus name of the service.
    pub destination: String,
    /// The object path.
    pub path: String,
    /// The interface.
    pub interface: String,
    /// The method.
    pub method: String,
    /// The signature of the arguments.
    pub signature: String,
    /// The arguments in the JSON form.
    pub args: Vec<Json>,
    /// Lets the service ask polkit for an interactive authentication.
    pub interactive: bool,
}

/// Does a call and returns the event for the shell.
pub async fn call(buses: &Buses, bus: Bus, id: u32, call: Call) -> DbusEvent {
    let error = |name: &str, message: String| DbusEvent::Error {
        id,
        name: name.to_owned(),
        message,
    };
    if !buses.options.allows(bus, &call.destination) {
        return error(
            ACCESS_DENIED,
            format!(
                "The shell cannot call {} on the {bus:?} bus.",
                call.destination
            ),
        );
    }
    let body = match arguments(&call.signature, &call.args) {
        Ok(body) => body,
        Err(message) => return error(INVALID_ARGS, message),
    };
    let connection = match buses.connection(bus).await {
        Ok(connection) => connection,
        Err(err) => return error(NO_BUS, err.to_string()),
    };
    match send(&connection, &call, body).await {
        Ok(Ok(values)) => DbusEvent::Reply { id, values },
        // The error of the service, with its name, for example org.freedesktop.DBus.Error.AccessDenied.
        Ok(Err((name, message))) => error(&name, message),
        Err(err) => error("org.freedesktop.DBus.Error.Failed", err.to_string()),
    }
}

/// The reply of a service: its values, or its error name and message.
type Reply = Result<Vec<Json>, (String, String)>;

/// Sends a call and waits for its reply. The outer error is a failure of the bus connection.
async fn send(
    connection: &Connection,
    call: &Call,
    body: Option<Structure<'static>>,
) -> zbus::Result<Reply> {
    let mut builder = zbus::message::Message::method_call(call.path.as_str(), call.method.as_str())
        .destination(call.destination.as_str())
        .interface(call.interface.as_str());
    if call.interactive {
        builder = builder.with_flags(Flags::ALLOW_INTERACTIVE_AUTH);
    }
    let message = match &body {
        Some(body) => builder.build(body)?,
        None => builder.build(&())?,
    };
    let serial = message.primary_header().serial_num();
    // The stream must exist before the send, thus it cannot miss the reply.
    let mut replies = MessageStream::from(connection.clone());
    connection.send(&message).await?;
    while let Some(reply) = replies.next().await {
        let reply = reply?;
        if reply.header().reply_serial() != Some(serial) {
            continue;
        }
        if reply.message_type() == MessageType::Error {
            let name = reply.header().error_name().map_or_else(
                || "org.freedesktop.DBus.Error.Failed".to_owned(),
                ToString::to_string,
            );
            // The first value of an error is its message, if the service gives one.
            let message = body_values(&reply)
                .ok()
                .and_then(|values| values.into_iter().next())
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            return Ok(Err((name, message)));
        }
        return Ok(Ok(body_values(&reply)?));
    }
    Err(zbus::Error::Failure(
        "the bus connection closed before the reply".to_owned(),
    ))
}

/// Returns the values of the body of a message in the JSON form.
fn body_values(message: &zbus::Message) -> zbus::Result<Vec<Json>> {
    let body = message.body();
    if matches!(body.signature(), Signature::Unit) {
        return Ok(Vec::new());
    }
    let values = match body.deserialize::<Structure<'_>>() {
        Ok(structure) => structure.fields().iter().map(to_json).collect(),
        // A body with one value that is not a structure.
        Err(_) => vec![to_json(&body.deserialize::<Value<'_>>()?)],
    };
    Ok(values)
}

/// Returns the body of a call: the arguments as a structure, or `None` for no arguments.
///
/// # Errors
///
/// A message if the signature is not valid or an argument does not fit it.
pub fn arguments(signature: &str, args: &[Json]) -> Result<Option<Structure<'static>>, String> {
    if signature.is_empty() {
        return if args.is_empty() {
            Ok(None)
        } else {
            Err("The call has arguments but no signature.".to_owned())
        };
    }
    let parsed = Signature::try_from(format!("({signature})").as_str())
        .map_err(|err| format!("The signature {signature} is not valid: {err}"))?;
    let Signature::Structure(fields) = parsed else {
        return Err(format!("The signature {signature} is not valid."));
    };
    let fields: Vec<&Signature> = fields.iter().collect();
    if fields.len() != args.len() {
        return Err(format!(
            "The signature {signature} has {} values, but the call has {} arguments.",
            fields.len(),
            args.len()
        ));
    }
    let mut builder = StructureBuilder::new();
    for (field, arg) in fields.into_iter().zip(args) {
        builder.push_value(from_json(field, arg)?);
    }
    builder.build().map(Some).map_err(|err| err.to_string())
}

/// Returns a D-Bus value of the type `signature` from its JSON form.
///
/// # Errors
///
/// A message if the JSON value does not fit the signature.
pub fn from_json(signature: &Signature, json: &Json) -> Result<Value<'static>, String> {
    let wrong = || format!("The value {json} is not a {signature}.");
    let int = |min: i128, max: i128| -> Result<i128, String> {
        let number = match json {
            Json::Number(number) => number
                .as_i64()
                .map(i128::from)
                .or_else(|| number.as_u64().map(i128::from)),
            // A 64-bit number can be larger than JavaScript can keep, thus it can come as text.
            Json::String(text) => text.parse::<i128>().ok(),
            _ => None,
        }
        .ok_or_else(wrong)?;
        if number < min || number > max {
            return Err(wrong());
        }
        Ok(number)
    };
    let text = || json.as_str().map(str::to_owned).ok_or_else(wrong);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(match signature {
        Signature::U8 => Value::U8(int(0, u8::MAX.into())? as u8),
        Signature::Bool => Value::Bool(json.as_bool().ok_or_else(wrong)?),
        Signature::I16 => Value::I16(int(i16::MIN.into(), i16::MAX.into())? as i16),
        Signature::U16 => Value::U16(int(0, u16::MAX.into())? as u16),
        Signature::I32 => Value::I32(int(i32::MIN.into(), i32::MAX.into())? as i32),
        Signature::U32 => Value::U32(int(0, u32::MAX.into())? as u32),
        Signature::I64 => Value::I64(int(i64::MIN.into(), i64::MAX.into())? as i64),
        Signature::U64 => Value::U64(int(0, u64::MAX.into())? as u64),
        Signature::F64 => Value::F64(json.as_f64().ok_or_else(wrong)?),
        Signature::Str => Value::Str(Str::from(text()?)),
        Signature::ObjectPath => {
            Value::ObjectPath(ObjectPath::try_from(text()?).map_err(|err| err.to_string())?)
        }
        Signature::Signature => {
            Value::Signature(Signature::try_from(text()?.as_str()).map_err(|err| err.to_string())?)
        }
        Signature::Variant => {
            let object = json.as_object().ok_or_else(wrong)?;
            let inner = object
                .get("type")
                .and_then(Json::as_str)
                .ok_or_else(wrong)?;
            let inner = Signature::try_from(inner).map_err(|err| err.to_string())?;
            let value = object.get("value").ok_or_else(wrong)?;
            Value::Variant(Box::new(from_json(&inner, value)?))
        }
        Signature::Array(child) => {
            let mut array = Array::new(child.signature());
            for element in json.as_array().ok_or_else(wrong)? {
                array
                    .append(from_json(child.signature(), element)?)
                    .map_err(|err| err.to_string())?;
            }
            Value::Array(array)
        }
        Signature::Dict { key, value } => {
            let mut dict = Dict::new(key.signature(), value.signature());
            for (name, element) in json.as_object().ok_or_else(wrong)? {
                // A JSON key is text. A number key comes as its text form.
                let key_json = match key.signature() {
                    Signature::Str | Signature::ObjectPath | Signature::Signature => {
                        Json::String(name.clone())
                    }
                    _ => serde_json::from_str(name).map_err(|_| wrong())?,
                };
                dict.append(
                    from_json(key.signature(), &key_json)?,
                    from_json(value.signature(), element)?,
                )
                .map_err(|err| err.to_string())?;
            }
            Value::Dict(dict)
        }
        Signature::Structure(fields) => {
            let elements = json.as_array().ok_or_else(wrong)?;
            let fields: Vec<&Signature> = fields.iter().collect();
            if fields.len() != elements.len() {
                return Err(wrong());
            }
            let mut builder = StructureBuilder::new();
            for (field, element) in fields.into_iter().zip(elements) {
                builder.push_value(from_json(field, element)?);
            }
            Value::Structure(builder.build().map_err(|err| err.to_string())?)
        }
        _ => {
            return Err(format!(
                "The shell cannot send a value of the type {signature}."
            ));
        }
    })
}

/// Returns the JSON form of a D-Bus value.
#[must_use]
pub fn to_json(value: &Value<'_>) -> Json {
    match value {
        Value::U8(n) => Json::from(*n),
        Value::Bool(b) => Json::Bool(*b),
        Value::I16(n) => Json::from(*n),
        Value::U16(n) => Json::from(*n),
        Value::I32(n) => Json::from(*n),
        Value::U32(n) => Json::from(*n),
        Value::I64(n) => Json::from(*n),
        Value::U64(n) => Json::from(*n),
        Value::F64(n) => Number::from_f64(*n).map_or(Json::Null, Json::Number),
        Value::Str(text) => Json::String(text.to_string()),
        Value::Signature(signature) => Json::String(signature.to_string()),
        Value::ObjectPath(path) => Json::String(path.to_string()),
        Value::Variant(inner) => {
            let mut object = Map::new();
            object.insert(
                "type".to_owned(),
                Json::String(inner.value_signature().to_string()),
            );
            object.insert("value".to_owned(), to_json(inner));
            Json::Object(object)
        }
        Value::Array(array) => Json::Array(array.iter().map(to_json).collect()),
        Value::Dict(dict) => {
            let mut object = Map::new();
            for (key, element) in dict.iter() {
                let key = match to_json(key) {
                    Json::String(text) => text,
                    other => other.to_string(),
                };
                object.insert(key, to_json(element));
            }
            Json::Object(object)
        }
        Value::Structure(structure) => {
            Json::Array(structure.fields().iter().map(to_json).collect())
        }
        #[allow(unreachable_patterns)]
        _ => Json::Null,
    }
}

/// A subscription filter of the shell.
#[derive(Clone, Debug)]
pub struct Filter {
    /// The bus name of the sender.
    pub sender: String,
    /// The object path.
    pub path: Option<String>,
    /// The interface.
    pub interface: Option<String>,
    /// The signal name.
    pub member: Option<String>,
}

/// Starts a subscription. The task sends each signal through `send` until it is aborted.
///
/// # Errors
///
/// A refused event for the shell if the allowlist refuses the sender or the bus is not available.
pub async fn subscribe(
    buses: &Buses,
    bus: Bus,
    id: u32,
    filter: Filter,
    send: impl Fn(DbusEvent) + Send + 'static,
) -> Result<JoinHandle<()>, DbusEvent> {
    let error = |name: &str, message: String| DbusEvent::Error {
        id,
        name: name.to_owned(),
        message,
    };
    if !buses.options.allows(bus, &filter.sender) {
        return Err(error(
            ACCESS_DENIED,
            format!(
                "The shell cannot get signals of {} on the {bus:?} bus.",
                filter.sender
            ),
        ));
    }
    let connection = buses
        .connection(bus)
        .await
        .map_err(|err| error(NO_BUS, err.to_string()))?;
    let mut builder = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(filter.sender.clone());
    if let Some(path) = filter.path.clone() {
        builder = builder.path(path);
    }
    if let Some(interface) = filter.interface.clone() {
        builder = builder.interface(interface);
    }
    if let Some(member) = filter.member.clone() {
        builder = builder.member(member);
    }
    let rule = builder
        .build()
        .map_err(|err| error(INVALID_ARGS, err.to_string()))?;
    let mut stream =
        MessageStream::for_match_rule(rule, &connection, std::num::NonZeroUsize::new(64))
            .await
            .map_err(|err| error("org.freedesktop.DBus.Error.Failed", err.to_string()))?;
    Ok(tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            let header = message.header();
            let event = DbusEvent::Signal {
                subscription: id,
                path: header.path().map(ToString::to_string).unwrap_or_default(),
                interface: header
                    .interface()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                member: header.member().map(ToString::to_string).unwrap_or_default(),
                values: body_values(&message).unwrap_or_default(),
            };
            send(event);
        }
    }))
}

/// The subscriptions of one channel. Dropping them ends the subscriptions.
#[derive(Debug, Default)]
pub struct Subscriptions(HashMap<u32, JoinHandle<()>>);

impl Subscriptions {
    /// Adds a subscription. A subscription with the same number ends.
    pub fn insert(&mut self, id: u32, task: JoinHandle<()>) {
        if let Some(old) = self.0.insert(id, task) {
            old.abort();
        }
    }

    /// Ends a subscription.
    pub fn remove(&mut self, id: u32) {
        if let Some(task) = self.0.remove(&id) {
            task.abort();
        }
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for task in self.0.values() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn allowlist_has_names_and_prefixes() {
        let options = DbusOptions {
            system: vec![
                "org.freedesktop.timedate1".to_owned(),
                "org.example.*".to_owned(),
            ],
            session: Vec::new(),
        };
        assert!(options.allows(Bus::System, "org.freedesktop.timedate1"));
        assert!(options.allows(Bus::System, "org.example.Service"));
        assert!(!options.allows(Bus::System, "org.example"));
        assert!(!options.allows(Bus::System, "org.exampleX.Service"));
        assert!(!options.allows(Bus::System, "org.freedesktop.timedate12"));
        assert!(!options.allows(Bus::Session, "org.freedesktop.timedate1"));
    }

    #[test]
    fn json_and_dbus_values_go_both_ways() {
        let cases = [
            ("s", json!("Europe/Vienna")),
            ("b", json!(true)),
            ("y", json!(255)),
            ("i", json!(-5)),
            ("t", json!(18_446_744_073_709_551_615_u64)),
            ("d", json!(1.5)),
            ("o", json!("/org/freedesktop/timedate1")),
            ("as", json!(["a", "b"])),
            (
                "a{sv}",
                json!({"Name": {"type": "s", "value": "x"}, "Count": {"type": "u", "value": 3}}),
            ),
            ("(sb)", json!(["x", false])),
            ("a{us}", json!({"1": "one"})),
            ("v", json!({"type": "ai", "value": [1, 2]})),
        ];
        for (signature, value) in cases {
            let parsed = Signature::try_from(signature).unwrap();
            let dbus =
                from_json(&parsed, &value).unwrap_or_else(|err| panic!("{signature}: {err}"));
            assert_eq!(dbus.value_signature().to_string(), signature);
            assert_eq!(to_json(&dbus), value, "{signature}");
        }
    }

    #[test]
    fn values_that_do_not_fit_are_refused() {
        for (signature, value) in [
            ("y", json!(256)),
            ("u", json!(-1)),
            ("s", json!(5)),
            ("b", json!("true")),
            ("(sb)", json!(["x"])),
            ("v", json!("no type")),
            ("o", json!("not a path")),
        ] {
            let parsed = Signature::try_from(signature).unwrap();
            assert!(from_json(&parsed, &value).is_err(), "{signature} {value}");
        }
    }

    #[test]
    fn a_64_bit_number_can_come_as_text() {
        let dbus = from_json(&Signature::U64, &json!("18446744073709551615")).unwrap();
        assert_eq!(dbus, Value::U64(u64::MAX));
    }

    #[test]
    fn arguments_check_the_number_of_values() {
        assert!(arguments("", &[]).unwrap().is_none());
        assert_eq!(
            arguments("sb", &[json!("x"), json!(true)])
                .unwrap()
                .unwrap()
                .fields()
                .len(),
            2
        );
        assert!(arguments("sb", &[json!("x")]).is_err());
        assert!(arguments("", &[json!("x")]).is_err());
        assert!(arguments("(", &[]).is_err());
    }
}
