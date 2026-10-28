//! Provider template snapshot loading and validation.
//!
//! The app ships `provider_templates.json` embedded via `include_str!`; this
//! module parses that snapshot (or a later network payload) into the public
//! [`ProviderTemplate`] model, dropping what the gateway cannot serve instead of
//! failing the whole document. Only structural problems (invalid JSON, an empty
//! or duplicate template id) are fatal.

mod propagation;
mod provider_ops;
mod snapshot;
mod source;
mod state;

pub use provider_ops::{
    apply_create_provider_from_template, apply_delete_provider_model,
    apply_restore_provider_model,
};
#[allow(unused_imports)]
pub use snapshot::{builtin_templates, find_builtin_template, parse_template_snapshot};
pub use state::{
    apply_delete_provider_template, apply_upsert_provider_template, provider_template_views,
    ProviderTemplateView,
};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use propagation::{
    apply_template_sync_from_body, apply_template_sync_with_notices,
    template_sync_addition_notices, ProviderRetirementNotice, TemplateAdditionNotice,
};
#[cfg(test)]
pub use propagation::apply_template_sync_with;
#[allow(unused_imports)]
pub(in crate::ai_gateway) use source::{fetch_template_models, fetch_template_models_with};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use state::{
    effective_provider_template, effective_quota_strategy, effective_template,
    effective_transport_strategy,
};
