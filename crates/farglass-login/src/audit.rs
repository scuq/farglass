//! The security log of farglass (ADR-043).
//!
//! Each security event goes to the tracing target `farglass::security` in one
//! fixed form, for the administrator and for fail2ban:
//!
//! ```text
//! event=login result=refused reason=address-cap addr=203.0.113.7 user=- uid=-
//! ```
//!
//! A success has the level info, a refusal the level warn, and a fault the
//! level error with `reason=fault` (ADR-042). No value has a space. No
//! password, PIN, recovery code, cookie, or ticket goes into the log.

use std::fmt::Write as _;

/// The tracing target of the security events.
pub const TARGET: &str = "farglass::security";

/// The result of a security event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The check passed.
    Accepted,
    /// The check refused the request.
    Refused,
    /// A fault of the host stopped the check, and the request was refused (ADR-042).
    Fault,
}

impl Outcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Refused | Self::Fault => "refused",
        }
    }
}

/// One security event.
#[derive(Clone, Debug, Default)]
pub struct Event<'a> {
    /// The name of the event, for example `login` or `connection`.
    pub event: &'a str,
    /// The cause, for example `address-cap`. Empty for a success with no cause.
    pub reason: &'a str,
    /// The address of the browser. Empty if unknown.
    pub addr: &'a str,
    /// The username, if known. It goes into the log only if it has safe characters.
    pub user: Option<&'a str>,
    /// The uid, if known.
    pub uid: Option<u32>,
    /// A detail for a fault, for example the error text. It comes after the fixed fields.
    pub detail: Option<&'a str>,
}

impl Event<'_> {
    /// Returns the log line of the event with the result `outcome`.
    #[must_use]
    pub fn line(&self, outcome: Outcome) -> String {
        let reason = match outcome {
            Outcome::Fault => "fault",
            _ if self.reason.is_empty() => "-",
            _ => self.reason,
        };
        let mut line = format!(
            "event={} result={} reason={} addr={} user={} uid={}",
            token(self.event),
            outcome.as_str(),
            token(reason),
            if self.addr.is_empty() {
                "-".to_owned()
            } else {
                token(self.addr)
            },
            self.user.map_or_else(|| "-".to_owned(), username),
            self.uid
                .map_or_else(|| "-".to_owned(), |uid| uid.to_string()),
        );
        if outcome == Outcome::Fault && !self.reason.is_empty() {
            let _ = write!(line, " cause={}", token(self.reason));
        }
        if let Some(detail) = self.detail {
            let _ = write!(line, " detail={detail:?}");
        }
        line
    }

    /// Logs the event as a success at level info.
    pub fn accepted(&self) {
        tracing::info!(target: TARGET, "{}", self.line(Outcome::Accepted));
    }

    /// Logs the event as a refusal at level warn.
    pub fn refused(&self) {
        tracing::warn!(target: TARGET, "{}", self.line(Outcome::Refused));
    }

    /// Logs the event as a refusal for a fault of the host at level error (ADR-042).
    pub fn fault(&self) {
        tracing::error!(target: TARGET, "{}", self.line(Outcome::Fault));
    }
}

/// Returns `value` with each character that is not safe in a log field replaced by `_`.
fn token(value: &str) -> String {
    value
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '/') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Returns the username for the log, or `?` if it has characters that a username of the host cannot have.
///
/// A username from the browser can hold any text, for example a password typed in the wrong field.
fn username(name: &str) -> String {
    let safe = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | '$'));
    if safe {
        name.to_owned()
    } else {
        "?".to_owned()
    }
}

/// Returns the group of an address for the limits: an IPv6 address gives its /64 network.
///
/// A host on IPv6 often has all addresses of a /64. Without the group, each of
/// them gets its own limits. An address that does not parse stays as it is.
#[must_use]
pub fn address_group(addr: &str) -> String {
    match addr.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(v6)) if v6.to_ipv4_mapped().is_none() => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Ok(std::net::IpAddr::V6(v6)) => v6.to_canonical().to_string(),
        Ok(ip) => ip.to_string(),
        Err(_) => addr.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_the_fixed_form() {
        let event = Event {
            event: "login",
            reason: "address-cap",
            addr: "203.0.113.7",
            ..Event::default()
        };
        assert_eq!(
            event.line(Outcome::Refused),
            "event=login result=refused reason=address-cap addr=203.0.113.7 user=- uid=-"
        );
        let event = Event {
            event: "login",
            addr: "2001:db8::1",
            user: Some("alice"),
            uid: Some(1000),
            ..Event::default()
        };
        assert_eq!(
            event.line(Outcome::Accepted),
            "event=login result=accepted reason=- addr=2001:db8::1 user=alice uid=1000"
        );
    }

    #[test]
    fn fault_has_reason_fault_and_the_cause() {
        let event = Event {
            event: "passkey",
            reason: "read-error",
            uid: Some(1000),
            detail: Some("No file descriptors"),
            ..Event::default()
        };
        assert_eq!(
            event.line(Outcome::Fault),
            "event=passkey result=refused reason=fault addr=- user=- uid=1000 cause=read-error detail=\"No file descriptors\""
        );
    }

    #[test]
    fn unsafe_values_cannot_break_the_form() {
        let event = Event {
            event: "login",
            reason: "x y",
            addr: "a b\nevent=fake",
            user: Some("my secret password"),
            ..Event::default()
        };
        let line = event.line(Outcome::Refused);
        assert_eq!(
            line,
            "event=login result=refused reason=x_y addr=a_b_event_fake user=? uid=-"
        );
        assert_eq!(line.matches("event=").count(), 1, "{line}");
    }

    #[test]
    fn ipv6_addresses_group_by_64() {
        assert_eq!(address_group("2001:db8:1:2:3:4:5:6"), "2001:db8:1:2::/64");
        assert_eq!(address_group("2001:db8:1:2::9"), "2001:db8:1:2::/64");
        assert_eq!(address_group("203.0.113.7"), "203.0.113.7");
        assert_eq!(address_group("::ffff:203.0.113.7"), "203.0.113.7");
        assert_eq!(address_group(""), "");
    }
}
