use super::{
    read_config, route_id_for_claude_provider, safe_id, AggregateRow, ProtocolRouterCallRecord,
    ProtocolRouterStatsSummary,
};
use std::collections::HashMap;

pub(in crate::protocol_router) fn summarize_calls(
    calls: Vec<ProtocolRouterCallRecord>,
) -> ProtocolRouterStatsSummary {
    let mut summary = ProtocolRouterStatsSummary {
        total_calls: calls.len(),
        ..ProtocolRouterStatsSummary::default()
    };
    for call in &calls {
        summary.input_tokens += call.input_tokens;
        summary.output_tokens += call.output_tokens;
        summary.total_tokens += call.total_tokens;
    }
    summary.by_route = aggregate(&calls, |call| call.route_id.clone());
    summary.by_provider = aggregate(&calls, |call| call.provider.clone());
    summary.by_model = aggregate(&calls, |call| call.model.clone());
    summary.calls = calls;
    summary
}

pub(in crate::protocol_router) fn aggregate(
    calls: &[ProtocolRouterCallRecord],
    key_fn: impl Fn(&ProtocolRouterCallRecord) -> String,
) -> Vec<AggregateRow> {
    let mut map: HashMap<String, AggregateRow> = HashMap::new();
    for call in calls {
        let key = key_fn(call);
        let row = map.entry(key.clone()).or_insert_with(|| AggregateRow {
            key,
            ..AggregateRow::default()
        });
        row.calls += 1;
        row.input_tokens += call.input_tokens;
        row.output_tokens += call.output_tokens;
        row.total_tokens += call.total_tokens;
    }
    let mut rows = map.into_values().collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.key.cmp(&b.key))
    });
    rows
}

pub(crate) fn router_base_url_for_route(route_id: &str) -> Result<String, String> {
    let config = read_config()?;
    Ok(format!(
        "http://127.0.0.1:{}/anthropic/{}/v1",
        config.port,
        safe_id(route_id)
    ))
}

pub(crate) fn router_base_url_for_claude_provider(provider_id: &str) -> Result<String, String> {
    router_base_url_for_route(&route_id_for_claude_provider(provider_id))
}

#[tauri::command]
pub fn protocol_router_base_url_for_claude_provider(provider_id: String) -> Result<String, String> {
    router_base_url_for_claude_provider(&provider_id)
}

pub(crate) fn router_token() -> Result<String, String> {
    Ok(read_config()?.token)
}
