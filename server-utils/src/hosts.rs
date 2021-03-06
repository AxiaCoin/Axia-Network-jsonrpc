//! Host header validation.

use crate::matcher::{Matcher, Pattern};
use std::collections::HashSet;
use std::net::SocketAddr;

const SPLIT_PROOF: &str = "split always returns non-empty iterator.";

/// Port pattern
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub enum Port {
	/// No port specified (default port)
	None,
	/// Port specified as a wildcard pattern
	Pattern(String),
	/// Fixed numeric port
	Fixed(u16),
}

impl From<Option<u16>> for Port {
	fn from(opt: Option<u16>) -> Self {
		match opt {
			Some(port) => Port::Fixed(port),
			None => Port::None,
		}
	}
}

impl From<u16> for Port {
	fn from(port: u16) -> Port {
		Port::Fixed(port)
	}
}

/// Host type
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct Host {
	hostname: String,
	port: Port,
	as_string: String,
	matcher: Matcher,
}

impl<T: AsRef<str>> From<T> for Host {
	fn from(string: T) -> Self {
		Host::parse(string.as_ref())
	}
}

impl Host {
	/// Creates a new `Host` given hostname and port number.
	pub fn new<T: Into<Port>>(hostname: &str, port: T) -> Self {
		let port = port.into();
		let hostname = Self::pre_process(hostname);
		let string = Self::to_string(&hostname, &port);
		let matcher = Matcher::new(&string);

		Host {
			hostname,
			port,
			as_string: string,
			matcher,
		}
	}

	/// Attempts to parse given string as a `Host`.
	/// NOTE: This method always succeeds and falls back to sensible defaults.
	pub fn parse(hostname: &str) -> Self {
		let hostname = Self::pre_process(hostname);
		let mut hostname = hostname.split(':');
		let host = hostname.next().expect(SPLIT_PROOF);
		let port = match hostname.next() {
			None => Port::None,
			Some(port) => match port.parse::<u16>().ok() {
				Some(num) => Port::Fixed(num),
				None => Port::Pattern(port.into()),
			},
		};

		Host::new(host, port)
	}

	fn pre_process(host: &str) -> String {
		// Remove possible protocol definition
		let mut it = host.split("://");
		let protocol = it.next().expect(SPLIT_PROOF);
		let host = match it.next() {
			Some(data) => data,
			None => protocol,
		};

		let mut it = host.split('/');
		it.next().expect(SPLIT_PROOF).to_lowercase()
	}

	fn to_string(hostname: &str, port: &Port) -> String {
		format!(
			"{}{}",
			hostname,
			match *port {
				Port::Fixed(port) => format!(":{}", port),
				Port::Pattern(ref port) => format!(":{}", port),
				Port::None => "".into(),
			},
		)
	}
}

impl Pattern for Host {
	fn matches<T: AsRef<str>>(&self, other: T) -> bool {
		self.matcher.matches(other)
	}
}

impl ::std::ops::Deref for Host {
	type Target = str;
	fn deref(&self) -> &Self::Target {
		&self.as_string
	}
}

/// Specifies if domains should be validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainsValidation<T> {
	/// Allow only domains on the list.
	AllowOnly(Vec<T>),
	/// Disable domains validation completely.
	Disabled,
}

impl<T> Into<Option<Vec<T>>> for DomainsValidation<T> {
	fn into(self) -> Option<Vec<T>> {
		use self::DomainsValidation::*;
		match self {
			AllowOnly(list) => Some(list),
			Disabled => None,
		}
	}
}

impl<T> From<Option<Vec<T>>> for DomainsValidation<T> {
	fn from(other: Option<Vec<T>>) -> Self {
		match other {
			Some(list) => DomainsValidation::AllowOnly(list),
			None => DomainsValidation::Disabled,
		}
	}
}

/// Returns `true` when `Host` header is whitelisted in `allowed_hosts`.
pub fn is_host_valid(host: Option<&str>, allowed_hosts: &Option<Vec<Host>>) -> bool {
	match allowed_hosts.as_ref() {
		None => true,
		Some(ref allowed_hosts) => match host {
			None => false,
			Some(ref host) => allowed_hosts.iter().any(|h| h.matches(host)),
		},
	}
}

/// Updates given list of hosts with the address.
pub fn update(hosts: Option<Vec<Host>>, address: &SocketAddr) -> Option<Vec<Host>> {
	use std::net::{IpAddr, Ipv4Addr};

	hosts.map(|current_hosts| {
		let mut new_hosts = current_hosts.into_iter().collect::<HashSet<_>>();
		let address_string = address.to_string();

		if address.ip() == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
			new_hosts.insert(address_string.replace("0.0.0.0", "127.0.0.1").into());
			new_hosts.insert(address_string.replace("0.0.0.0", "localhost").into());
		} else if address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST) {
			new_hosts.insert(address_string.replace("127.0.0.1", "localhost").into());
		}

		new_hosts.insert(address_string.into());
		new_hosts.into_iter().collect()
	})
}

#[cfg(test)]
mod tests {
	use super::{is_host_valid, Host};

	#[test]
	fn should_parse_host() {
		assert_eq!(Host::parse("http://axiacoin.network"), Host::new("axiacoin.network", None));
		assert_eq!(
			Host::parse("https://axiacoin.network:8443"),
			Host::new("axiacoin.network", Some(8443))
		);
		assert_eq!(
			Host::parse("chrome-extension://124.0.0.1"),
			Host::new("124.0.0.1", None)
		);
		assert_eq!(Host::parse("axiacoin.network/somepath"), Host::new("axiacoin.network", None));
		assert_eq!(
			Host::parse("127.0.0.1:8545/somepath"),
			Host::new("127.0.0.1", Some(8545))
		);
	}

	#[test]
	fn should_reject_when_there_is_no_header() {
		let valid = is_host_valid(None, &Some(vec![]));
		assert_eq!(valid, false);
	}

	#[test]
	fn should_reject_when_validation_is_disabled() {
		let valid = is_host_valid(Some("any"), &None);
		assert_eq!(valid, true);
	}

	#[test]
	fn should_reject_if_header_not_on_the_list() {
		let valid = is_host_valid(Some("axiacoin.network"), &Some(vec![]));
		assert_eq!(valid, false);
	}

	#[test]
	fn should_accept_if_on_the_list() {
		let valid = is_host_valid(Some("axiacoin.network"), &Some(vec!["axiacoin.network".into()]));
		assert_eq!(valid, true);
	}

	#[test]
	fn should_accept_if_on_the_list_with_port() {
		let valid = is_host_valid(Some("axiacoin.network:443"), &Some(vec!["axiacoin.network:443".into()]));
		assert_eq!(valid, true);
	}

	#[test]
	fn should_support_wildcards() {
		let valid = is_host_valid(Some("axia.web3.site:8180"), &Some(vec!["*.web3.site:*".into()]));
		assert_eq!(valid, true);
	}
}
