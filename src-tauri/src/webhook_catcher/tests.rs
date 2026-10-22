#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::webhook_catcher::signature::{detect_body_format, detect_signature};
    use crate::webhook_catcher::storage::{
        append_captured_request, clear_captured_requests, get_captured_requests,
    };
    use crate::webhook_catcher::types::CapturedWebhookRequest;

    #[test]
    fn test_signature_detection() {
        let mut headers = HashMap::new();
        headers.insert(
            "X-Hub-Signature-256".to_string(),
            "sha256=123456abcdef".to_string(),
        );

        let detected = detect_signature(&headers).expect("should detect github signature");
        assert_eq!(detected.provider, "GitHub");
        assert_eq!(detected.header_name, "X-Hub-Signature-256");

        let mut stripe_headers = HashMap::new();
        stripe_headers.insert("Stripe-Signature".to_string(), "t=123,v1=abc".to_string());
        let stripe_sig = detect_signature(&stripe_headers).expect("should detect stripe");
        assert_eq!(stripe_sig.provider, "Stripe");
    }

    #[test]
    fn test_body_format_detection() {
        assert_eq!(
            detect_body_format("application/json", "{\"key\": \"val\"}"),
            "json"
        );
        assert_eq!(
            detect_body_format("text/plain", "{\"auto_detect\": true}"),
            "json"
        );
        assert_eq!(
            detect_body_format("application/x-www-form-urlencoded", "a=1&b=2"),
            "form"
        );
        assert_eq!(
            detect_body_format("text/xml", "<xml><val>1</val></xml>"),
            "xml"
        );
        assert_eq!(
            detect_body_format("text/plain", "just plain text message"),
            "raw"
        );
    }

    #[test]
    fn test_captured_requests_queue_and_filtering() {
        clear_captured_requests(None);

        let ep1 = "ep-1";
        let ep2 = "ep-2";

        for i in 0..5 {
            let req = CapturedWebhookRequest {
                id: format!("req-{}", i),
                endpoint_id: ep1.to_string(),
                endpoint_name: "Test Ep 1".to_string(),
                timestamp: 1000 + i,
                method: "POST".to_string(),
                path: "/hook/test".to_string(),
                query_params: HashMap::new(),
                headers: HashMap::new(),
                client_ip: "127.0.0.1".to_string(),
                body: "{}".to_string(),
                body_format: "json".to_string(),
                is_truncated: false,
                detected_signature: None,
            };
            append_captured_request(req);
        }

        let req_ep2 = CapturedWebhookRequest {
            id: "req-ep2".to_string(),
            endpoint_id: ep2.to_string(),
            endpoint_name: "Test Ep 2".to_string(),
            timestamp: 2000,
            method: "GET".to_string(),
            path: "/hook/ep2".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            client_ip: "127.0.0.1".to_string(),
            body: "".to_string(),
            body_format: "raw".to_string(),
            is_truncated: false,
            detected_signature: None,
        };
        append_captured_request(req_ep2);

        // 全量查询应有 6 条
        let all = get_captured_requests(None, None);
        assert_eq!(all.len(), 6);

        // 按 endpoint_id 筛选
        let ep1_reqs = get_captured_requests(Some(ep1), None);
        assert_eq!(ep1_reqs.len(), 5);

        let ep2_reqs = get_captured_requests(Some(ep2), None);
        assert_eq!(ep2_reqs.len(), 1);

        // 清空 ep1
        clear_captured_requests(Some(ep1));
        let ep1_after = get_captured_requests(Some(ep1), None);
        assert_eq!(ep1_after.len(), 0);

        let ep2_after = get_captured_requests(Some(ep2), None);
        assert_eq!(ep2_after.len(), 1);

        // 全量清空
        clear_captured_requests(None);
        assert_eq!(get_captured_requests(None, None).len(), 0);
    }
}
