#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayErrorCategory {
    StorageUnavailable,
    CredentialMissing,
    CredentialStoreUnavailable,
    CredentialInvalid,
    CredentialAuthenticationFailed,
    CredentialVersionUnsupported,
}

impl GatewayErrorCategory {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::StorageUnavailable => "storage_unavailable",
            Self::CredentialMissing => "credential_missing",
            Self::CredentialStoreUnavailable => "credential_store_unavailable",
            Self::CredentialInvalid => "credential_invalid",
            Self::CredentialAuthenticationFailed => "credential_authentication_failed",
            Self::CredentialVersionUnsupported => "credential_version_unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayError {
    category: GatewayErrorCategory,
    entity_id: Option<String>,
}

impl GatewayError {
    pub(crate) fn new(category: GatewayErrorCategory, entity_id: Option<&str>) -> Self {
        Self {
            category,
            entity_id: entity_id.map(str::to_owned),
        }
    }

    pub(crate) fn category(&self) -> GatewayErrorCategory {
        self.category
    }

    pub(crate) fn entity_id(&self) -> Option<&str> {
        self.entity_id.as_deref()
    }
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.category.code())?;
        if let Some(entity_id) = &self.entity_id {
            write!(formatter, ":{entity_id}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GatewayError {}
