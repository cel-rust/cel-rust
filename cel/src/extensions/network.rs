//! The network extension library: the `net.IP` and `net.CIDR` types and
//! their functions, following cel-go's `ext.Network()`.

use crate::common::types::{CelBool, CelInt, CelString, Kind, Type};
use crate::common::value::{StaticVal, Val};
use crate::{DeclarationError, Env, ExecutionError, Library};
use std::any::Any;
use std::net::IpAddr;

static IP_TYPE: Type = Type::simple_type(Kind::Opaque, "net.IP");
static CIDR_TYPE: Type = Type::simple_type(Kind::Opaque, "net.CIDR");

/// Adds the network extension library to `env`: [`Network`], with its default
/// settings.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    env.add_library(Network::default())
}

/// The network extension library, `cel.lib.ext.network`: registers the
/// `net.IP` and `net.CIDR` types and their overloads on the [`Env`] it is
/// added to.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Network {}

impl Library for Network {
    fn name(&self) -> Option<&str> {
        Some("cel.lib.ext.network")
    }

    fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
        env.add_type(IP_TYPE.to_owned())?;
        env.add_type(CIDR_TYPE.to_owned())?;

        crate::add_overload!(env, fn string_to_ip: (CelString) -> Result<Ip>, name = "ip")?;
        crate::add_overload!(env, fn ip_to_string: (Ip) -> CelString, name = "string")?;
        crate::add_overload!(env, fn is_ip: (CelString) -> CelBool, name = "isIP")?;
        crate::add_overload!(env, fn is_canonical: (CelString) -> Result<CelBool>,
            name = "ip.isCanonical")?;
        crate::add_member_overload!(env, fn family: (Ip) -> CelInt)?;
        crate::add_member_overload!(env, fn is_unspecified: (Ip) -> CelBool)?;
        crate::add_member_overload!(env, fn is_loopback: (Ip) -> CelBool)?;
        crate::add_member_overload!(env, fn is_global_unicast: (Ip) -> CelBool)?;
        crate::add_member_overload!(env, fn is_link_local_multicast: (Ip) -> CelBool)?;
        crate::add_member_overload!(env, fn is_link_local_unicast: (Ip) -> CelBool)?;

        crate::add_overload!(env, fn string_to_cidr: (CelString) -> Result<Cidr>, name = "cidr")?;
        crate::add_overload!(env, fn cidr_to_string: (Cidr) -> CelString, name = "string")?;
        crate::add_overload!(env, fn is_cidr: (CelString) -> CelBool, name = "isCIDR")?;
        crate::add_member_overload!(env, fn contains_ip: (Cidr, Ip) -> CelBool, name = "containsIP")?;
        crate::add_member_overload!(env, fn contains_ip_string: (Cidr, CelString) -> Result<CelBool>,
            name = "containsIP")?;
        crate::add_member_overload!(env, fn contains_cidr: (Cidr, Cidr) -> CelBool,
            name = "containsCIDR")?;
        crate::add_member_overload!(env, fn contains_cidr_string: (Cidr, CelString) -> Result<CelBool>,
            name = "containsCIDR")?;
        crate::add_member_overload!(env, fn cidr_ip: (Cidr) -> Ip, name = "ip")?;
        crate::add_member_overload!(env, fn masked: (Cidr) -> Cidr)?;
        crate::add_member_overload!(env, fn is_mask: (Cidr) -> CelBool)?;
        crate::add_member_overload!(env, fn prefix_length: (Cidr) -> CelInt)?;
        Ok(())
    }
}

/// A `net.IP`: an IPv4 or IPv6 address, without a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ip(IpAddr);

/// A `net.CIDR`: an address and a prefix length, the address kept as written,
/// host bits included, as Go's `netip.Prefix` keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    bits: u8,
}

impl Val for Ip {
    fn get_type(&self) -> &Type {
        &IP_TYPE
    }

    fn cel_type() -> &'static Type {
        &IP_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Ip>() == Some(self)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
        Box::new(*self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Ip {}

impl Val for Cidr {
    fn get_type(&self) -> &Type {
        &CIDR_TYPE
    }

    fn cel_type() -> &'static Type {
        &CIDR_TYPE
    }

    fn equals(&self, other: &dyn Val) -> bool {
        other.downcast_ref::<Cidr>() == Some(self)
    }

    fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v> {
        Box::new(*self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl StaticVal for Cidr {}

/// Parses an address as cel-go does: no zone, and no IPv4-mapped IPv6
/// address, which would otherwise be a second spelling of an IPv4 one.
/// Errors read as the conformance tests expect them.
fn parse_ip(raw: &str) -> Result<IpAddr, String> {
    if raw.contains('%') {
        return Err("IP Address with zone value is not allowed".to_owned());
    }
    let addr = raw.parse::<IpAddr>().map_err(|e| {
        format!("IP Address '{raw}' parse error during conversion from string: {e}")
    })?;
    if matches!(addr, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some()) {
        return Err("IPv4-mapped IPv6 address is not allowed".to_owned());
    }
    Ok(addr)
}

/// Parses `addr/bits` with the same rules as [`parse_ip`], and a prefix
/// length in decimal, without leading zeros, as Go's `netip.ParsePrefix`.
fn parse_cidr(raw: &str) -> Result<Cidr, String> {
    let invalid =
        |why: &str| format!("network address parse error during conversion from string: {why}");
    let (addr, bits) = raw.rsplit_once('/').ok_or_else(|| invalid("no '/'"))?;
    if addr.contains('%') {
        return Err("CIDR with zone value is not allowed".to_owned());
    }
    let addr = addr
        .parse::<IpAddr>()
        .map_err(|e| invalid(&e.to_string()))?;
    if matches!(addr, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some()) {
        return Err("IPv4-mapped IPv6 address is not allowed".to_owned());
    }
    let max = if addr.is_ipv4() { 32 } else { 128 };
    let bits = match bits.parse::<u8>() {
        // `u8::from_str` takes a `+` sign, and leading zeros
        Ok(b) if b <= max && (bits == "0" || !bits.starts_with(['0', '+'])) => b,
        _ => return Err(invalid("bad bits")),
    };
    Ok(Cidr { addr, bits })
}

/// `addr` with all but its first `bits` bits zeroed.
fn mask(addr: IpAddr, bits: u8) -> IpAddr {
    match addr {
        IpAddr::V4(v4) => {
            let m = u32::MAX.checked_shl(32 - bits as u32).unwrap_or(0);
            IpAddr::V4((u32::from(v4) & m).into())
        }
        IpAddr::V6(v6) => {
            let m = u128::MAX.checked_shl(128 - bits as u32).unwrap_or(0);
            IpAddr::V6((u128::from(v6) & m).into())
        }
    }
}

impl Cidr {
    fn contains(&self, addr: IpAddr) -> bool {
        addr.is_ipv4() == self.addr.is_ipv4() && mask(addr, self.bits) == mask(self.addr, self.bits)
    }

    fn contains_cidr(&self, other: &Cidr) -> bool {
        self.bits <= other.bits && self.contains(other.addr)
    }
}

fn string_to_ip(raw: &CelString<'_>) -> Result<Ip, ExecutionError> {
    parse_ip(raw.inner())
        .map(Ip)
        .map_err(|e| ExecutionError::function_error("ip", e))
}

fn ip_to_string(this: &Ip) -> CelString<'static> {
    this.0.to_string().into()
}

fn is_ip(raw: &CelString<'_>) -> CelBool {
    parse_ip(raw.inner()).is_ok().into()
}

fn is_canonical(raw: &CelString<'_>) -> Result<CelBool, ExecutionError> {
    let addr =
        parse_ip(raw.inner()).map_err(|e| ExecutionError::function_error("ip.isCanonical", e))?;
    Ok((addr.to_string() == raw.inner()).into())
}

fn family(this: &Ip) -> CelInt {
    if this.0.is_ipv4() { 4 } else { 6 }.into()
}

fn is_unspecified(this: &Ip) -> CelBool {
    this.0.is_unspecified().into()
}

fn is_loopback(this: &Ip) -> CelBool {
    this.0.is_loopback().into()
}

/// As Go's `netip.Addr.IsGlobalUnicast`: private addresses are global
/// unicast too.
fn is_global_unicast(this: &Ip) -> CelBool {
    let local = match this.0 {
        IpAddr::V4(v4) => v4.is_broadcast() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.segments()[0] & 0xffc0 == 0xfe80,
    };
    (!local && !this.0.is_unspecified() && !this.0.is_loopback() && !this.0.is_multicast()).into()
}

fn is_link_local_multicast(this: &Ip) -> CelBool {
    match this.0 {
        IpAddr::V4(v4) => v4.octets()[..3] == [224, 0, 0],
        IpAddr::V6(v6) => v6.segments()[0] & 0xff0f == 0xff02,
    }
    .into()
}

fn is_link_local_unicast(this: &Ip) -> CelBool {
    match this.0 {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => v6.segments()[0] & 0xffc0 == 0xfe80,
    }
    .into()
}

fn string_to_cidr(raw: &CelString<'_>) -> Result<Cidr, ExecutionError> {
    parse_cidr(raw.inner()).map_err(|e| ExecutionError::function_error("cidr", e))
}

fn cidr_to_string(this: &Cidr) -> CelString<'static> {
    format!("{}/{}", this.addr, this.bits).into()
}

fn is_cidr(raw: &CelString<'_>) -> CelBool {
    parse_cidr(raw.inner()).is_ok().into()
}

fn contains_ip(this: &Cidr, ip: &Ip) -> CelBool {
    this.contains(ip.0).into()
}

fn contains_ip_string(this: &Cidr, raw: &CelString<'_>) -> Result<CelBool, ExecutionError> {
    let addr =
        parse_ip(raw.inner()).map_err(|e| ExecutionError::function_error("containsIP", e))?;
    Ok(this.contains(addr).into())
}

fn contains_cidr(this: &Cidr, other: &Cidr) -> CelBool {
    this.contains_cidr(other).into()
}

fn contains_cidr_string(this: &Cidr, raw: &CelString<'_>) -> Result<CelBool, ExecutionError> {
    let other =
        parse_cidr(raw.inner()).map_err(|e| ExecutionError::function_error("containsCIDR", e))?;
    Ok(this.contains_cidr(&other).into())
}

fn cidr_ip(this: &Cidr) -> Ip {
    Ip(this.addr)
}

fn masked(this: &Cidr) -> Cidr {
    Cidr {
        addr: mask(this.addr, this.bits),
        bits: this.bits,
    }
}

fn is_mask(this: &Cidr) -> CelBool {
    (mask(this.addr, this.bits) == this.addr).into()
}

fn prefix_length(this: &Cidr) -> CelInt {
    i64::from(this.bits).into()
}

#[cfg(test)]
mod tests {
    use crate::{Context, Env, ExecutionError, Value};
    use std::sync::Arc;

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::network)
            .expect("We can't test the extension, if we can't register it");
        let program = env.compile(expr).expect("This must be valid CEL");
        program.execute(&Context::with_env(Arc::new(env)))
    }

    #[test]
    fn prefix_lengths_are_decimal_without_sign_or_leading_zero() {
        for valid in ["10.0.0.0/0", "10.0.0.0/8", "10.0.0.0/32", "::/128"] {
            assert_eq!(
                eval(&format!("isCIDR('{valid}')")),
                Ok(Value::Bool(true)),
                "{valid}"
            );
        }
        for invalid in [
            "10.0.0.0",
            "10.0.0.0/",
            "10.0.0.0/33",
            "::/129",
            "10.0.0.0/08",
            "10.0.0.0/+8",
            "10.0.0.0/-1",
        ] {
            assert_eq!(
                eval(&format!("isCIDR('{invalid}')")),
                Ok(Value::Bool(false)),
                "{invalid}"
            );
        }
    }

    #[test]
    fn a_cidr_keeps_its_host_bits_until_masked() {
        assert_eq!(
            eval("string(cidr('10.1.2.3/8'))"),
            Ok(Value::from("10.1.2.3/8"))
        );
        assert_eq!(eval("cidr('10.1.2.3/8').isMask()"), Ok(Value::Bool(false)));
        assert_eq!(
            eval("cidr('10.1.2.3/8').masked().isMask()"),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            eval("cidr('10.0.0.0/8') == cidr('10.1.2.3/8')"),
            Ok(Value::Bool(false))
        );
        assert_eq!(
            eval("cidr('::/0').containsIP('2001:db8::1')"),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            eval("cidr('0.0.0.0/0').containsIP('::')"),
            Ok(Value::Bool(false))
        );
        assert_eq!(
            eval("cidr('0.0.0.0/0').containsCIDR('::/0')"),
            Ok(Value::Bool(false))
        );
    }

    #[test]
    fn ipv4_mapped_addresses_are_rejected_in_any_spelling() {
        for raw in ["::ffff:192.168.0.1", "::ffff:c0a8:1"] {
            assert_eq!(
                eval(&format!("isIP('{raw}')")),
                Ok(Value::Bool(false)),
                "{raw}"
            );
        }
        assert!(matches!(
            eval("ip('::ffff:c0a8:1')"),
            Err(ExecutionError::FunctionError { function, .. }) if function == "ip"
        ));
    }
}
