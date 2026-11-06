#[cfg(test)]
mod tests {
    use crate::dns_inspector::doh::type_code_to_name;
    use crate::dns_inspector::tcp_ping::analyze_ip;

    #[test]
    fn test_type_code_mapping() {
        assert_eq!(type_code_to_name(1), "A");
        assert_eq!(type_code_to_name(28), "AAAA");
        assert_eq!(type_code_to_name(5), "CNAME");
        assert_eq!(type_code_to_name(16), "TXT");
        assert_eq!(type_code_to_name(15), "MX");
    }

    #[test]
    fn test_ip_classification() {
        let loopback = analyze_ip("127.0.0.1");
        assert!(loopback.is_loopback);

        let priv_v4 = analyze_ip("192.168.1.1");
        assert!(priv_v4.is_private);
        assert!(!priv_v4.is_loopback);

        let public_v4 = analyze_ip("1.1.1.1");
        assert!(!public_v4.is_private);
        assert!(!public_v4.is_loopback);
    }
}
