pub mod doh;
pub mod tcp_ping;
pub mod commands;

pub use commands::{
    dns_inspector_query,
    dns_inspector_tcp_ping,
    dns_inspector_tcp_ping_stream_start,
    dns_inspector_tcp_ping_stream_stop,
    dns_inspector_ip_lookup,
};

#[cfg(test)]
mod tests;
