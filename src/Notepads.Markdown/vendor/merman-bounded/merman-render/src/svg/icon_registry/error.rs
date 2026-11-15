use super::IconRegistryResourceLimitId;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IconRegistryBuildErrorKind {
    ResourceLimitExceeded,
    InvalidUtf8,
    InvalidJson,
    DuplicateJsonKey,
    InvalidSchema,
    InvalidIdentifier,
    InvalidGeometry,
    InvalidXml,
    DuplicateIcon,
    AliasCycle,
    MissingAliasParent,
    ArithmeticOverflow,
    AllocationFailed,
}

impl IconRegistryBuildErrorKind {
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::ResourceLimitExceeded => "resource_limit_exceeded",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::InvalidJson => "invalid_json",
            Self::DuplicateJsonKey => "duplicate_json_key",
            Self::InvalidSchema => "invalid_schema",
            Self::InvalidIdentifier => "invalid_identifier",
            Self::InvalidGeometry => "invalid_geometry",
            Self::InvalidXml => "invalid_xml",
            Self::DuplicateIcon => "duplicate_icon",
            Self::AliasCycle => "alias_cycle",
            Self::MissingAliasParent => "missing_alias_parent",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::AllocationFailed => "allocation_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRegistryBuildError {
    kind: IconRegistryBuildErrorKind,
    pack_index: Option<usize>,
    registration_name: Option<String>,
    limit: Option<IconRegistryResourceLimitId>,
    actual: Option<u64>,
    maximum: Option<u64>,
    message: String,
}

impl IconRegistryBuildError {
    pub(super) fn new(
        kind: IconRegistryBuildErrorKind,
        pack_index: impl Into<Option<usize>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            pack_index: pack_index.into(),
            registration_name: None,
            limit: None,
            actual: None,
            maximum: None,
            message: message.into(),
        }
    }

    pub(super) fn with_registration_name(mut self, registration_name: Option<&str>) -> Self {
        self.registration_name = registration_name.map(str::to_owned);
        self
    }

    pub(super) fn with_limit(
        mut self,
        limit: IconRegistryResourceLimitId,
        actual: impl TryInto<u64>,
        maximum: impl TryInto<u64>,
    ) -> Self {
        self.limit = Some(limit);
        self.actual = actual.try_into().ok();
        self.maximum = maximum.try_into().ok();
        self
    }

    pub const fn kind(&self) -> IconRegistryBuildErrorKind {
        self.kind
    }

    pub const fn pack_index(&self) -> Option<usize> {
        self.pack_index
    }

    pub fn registration_name(&self) -> Option<&str> {
        self.registration_name.as_deref()
    }

    pub const fn limit_id(&self) -> Option<&'static str> {
        match self.limit {
            Some(limit) => Some(limit.stable_id()),
            None => None,
        }
    }

    pub const fn limit(&self) -> Option<IconRegistryResourceLimitId> {
        self.limit
    }

    pub const fn actual(&self) -> Option<u64> {
        self.actual
    }

    pub const fn maximum(&self) -> Option<u64> {
        self.maximum
    }
}

impl fmt::Display for IconRegistryBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "icon registry build failed ({})",
            self.kind.stable_id()
        )?;
        if let Some(pack_index) = self.pack_index {
            write!(formatter, " in pack {pack_index}")?;
        }
        if let Some(registration_name) = &self.registration_name {
            write!(formatter, " registered as `{registration_name}`")?;
        }
        write!(formatter, ": {}", self.message)?;
        if let Some(limit) = self.limit {
            write!(formatter, " [{}", limit.stable_id())?;
            if let Some(actual) = self.actual {
                write!(formatter, " actual={actual}")?;
            }
            if let Some(maximum) = self.maximum {
                write!(formatter, " max={maximum}")?;
            }
            formatter.write_str("]")?;
        }
        Ok(())
    }
}

impl std::error::Error for IconRegistryBuildError {}
