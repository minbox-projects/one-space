#[cfg(test)]
mod tests {
    use url::Url;

    #[test]
    fn test_relative_location_resolution() {
        let base = Url::parse("https://example.com/login?step=1").unwrap();
        let next = base.join("/dashboard?auth=ok").unwrap();
        assert_eq!(next.as_str(), "https://example.com/dashboard?auth=ok");

        let next_relative = base.join("welcome").unwrap();
        assert_eq!(next_relative.as_str(), "https://example.com/welcome");

        let next_absolute = base.join("https://other.com/cb").unwrap();
        assert_eq!(next_absolute.as_str(), "https://other.com/cb");
    }
}
