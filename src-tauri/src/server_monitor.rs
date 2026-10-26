//! Direct Linux resource collection over owned SSH sessions.
//!
//! The module owns the fixed collector script, the pure parser/calculator for
//! its framed output and the session lifetime policy. Remote hosts run only the
//! repository-owned `/bin/sh -s` script; no installer, agent or remotely
//! interpolated command text is involved.

pub(crate) mod metrics;
pub(crate) mod transport;
pub(crate) mod types;

#[cfg(test)]
mod tests;
