/// Shared utilities for normalizing ports/bind addresses.
///
/// We intentionally keep the logic here minimal and dependency-free so both the library
/// (web/prom servers) and the binary (CLI/config parsing) can use the same behavior.
///
/// Supported input examples:
/// - ":3030"          -> ":3030"
/// - "3030"           -> ":3030"
/// - "127.0.0.1:3030" -> "127.0.0.1:3030"
/// - "0.0.0.0:3030"   -> "0.0.0.0:3030"
pub fn normalize_port(port_or_addr: &str) -> String {
    let s = port_or_addr.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.starts_with(':') {
        s.to_string()
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        format!(":{}", s)
    } else {
        s.to_string()
    }
}

/// Convert a port-or-address string into a concrete bind address suitable for `SocketAddr::parse()`.
///
/// Used for **Stratum** listeners. Port-only forms (`:5555`, `5555`) bind to **`0.0.0.0`** so miners
/// on the LAN can connect. For dashboard/metrics HTTP, use [`bind_addr_for_operator_http`] instead.
pub fn bind_addr_from_port(port_or_addr: &str) -> String {
    let s = normalize_port(port_or_addr);
    if s.is_empty() {
        return s;
    }
    if s.starts_with(':') { format!("0.0.0.0{}", s) } else { s }
}

/// Bind address for **web dashboard** and **per-instance Prometheus HTTP** (operator-facing).
///
/// Port-only config (`:3030`, `3030`) defaults to **`127.0.0.1`** so a typical home setup does not
/// expose `/api/*` and `/metrics` to the whole LAN or internet.
///
/// To listen on all interfaces (e.g. open the dashboard from another device on your network), set an
/// explicit address such as `0.0.0.0:3030` or `192.168.1.10:3030`.
pub fn bind_addr_for_operator_http(port_or_addr: &str) -> String {
    let s = normalize_port(port_or_addr);
    if s.is_empty() {
        return s;
    }
    if s.starts_with(':') { format!("127.0.0.1{}", s) } else { s }
}
