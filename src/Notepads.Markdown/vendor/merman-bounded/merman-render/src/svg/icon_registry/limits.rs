const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IconRegistryResourceLimitId {
    MaxPacks,
    MaxPackBytes,
    MaxInputBytes,
    MaxJsonDepth,
    MaxJsonMembers,
    MaxJsonKeyBytes,
    MaxPrefixBytes,
    MaxNameBytes,
    MaxBodyBytes,
    MaxRetainedBodyBytes,
    MaxIconEntries,
    MaxAliasEntries,
    MaxTotalEntries,
    MaxAliasEdges,
    MaxAliasDepth,
    MaxAliasFanout,
    MaxBuildWorkUnits,
    MaxXmlElementsPerBody,
    MaxXmlDepthPerBody,
    MaxIdRewriteEditsPerBody,
    MaxRetainedXmlPlanBytes,
    MaxCoordinateMagnitude,
}

impl IconRegistryResourceLimitId {
    pub const ALL: &'static [Self] = &[
        Self::MaxPacks,
        Self::MaxPackBytes,
        Self::MaxInputBytes,
        Self::MaxJsonDepth,
        Self::MaxJsonMembers,
        Self::MaxJsonKeyBytes,
        Self::MaxPrefixBytes,
        Self::MaxNameBytes,
        Self::MaxBodyBytes,
        Self::MaxRetainedBodyBytes,
        Self::MaxIconEntries,
        Self::MaxAliasEntries,
        Self::MaxTotalEntries,
        Self::MaxAliasEdges,
        Self::MaxAliasDepth,
        Self::MaxAliasFanout,
        Self::MaxBuildWorkUnits,
        Self::MaxXmlElementsPerBody,
        Self::MaxXmlDepthPerBody,
        Self::MaxIdRewriteEditsPerBody,
        Self::MaxRetainedXmlPlanBytes,
        Self::MaxCoordinateMagnitude,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::MaxPacks => "max_icon_registry_packs",
            Self::MaxPackBytes => "max_icon_pack_bytes",
            Self::MaxInputBytes => "max_icon_registry_input_bytes",
            Self::MaxJsonDepth => "max_icon_registry_json_depth",
            Self::MaxJsonMembers => "max_icon_registry_json_members",
            Self::MaxJsonKeyBytes => "max_icon_registry_json_key_bytes",
            Self::MaxPrefixBytes => "max_icon_registry_prefix_bytes",
            Self::MaxNameBytes => "max_icon_registry_name_bytes",
            Self::MaxBodyBytes => "max_icon_body_bytes",
            Self::MaxRetainedBodyBytes => "max_icon_registry_retained_body_bytes",
            Self::MaxIconEntries => "max_icon_registry_icon_entries",
            Self::MaxAliasEntries => "max_icon_registry_alias_entries",
            Self::MaxTotalEntries => "max_icon_registry_entries",
            Self::MaxAliasEdges => "max_icon_registry_alias_edges",
            Self::MaxAliasDepth => "max_icon_registry_alias_depth",
            Self::MaxAliasFanout => "max_icon_registry_alias_fanout",
            Self::MaxBuildWorkUnits => "max_icon_registry_build_work_units",
            Self::MaxXmlElementsPerBody => "max_icon_xml_elements_per_body",
            Self::MaxXmlDepthPerBody => "max_icon_xml_depth_per_body",
            Self::MaxIdRewriteEditsPerBody => "max_icon_id_rewrite_edits_per_body",
            Self::MaxRetainedXmlPlanBytes => "max_icon_registry_retained_xml_plan_bytes",
            Self::MaxCoordinateMagnitude => "max_icon_coordinate_magnitude",
        }
    }

    pub const fn fixed_value(self) -> u64 {
        match self {
            Self::MaxPacks => 16,
            Self::MaxPackBytes => 16 * MIB,
            Self::MaxInputBytes => 32 * MIB,
            Self::MaxJsonDepth => 32,
            Self::MaxJsonMembers => 1_000_000,
            Self::MaxJsonKeyBytes => 1_024,
            Self::MaxPrefixBytes => 64,
            Self::MaxNameBytes => 128,
            Self::MaxBodyBytes => 256 * KIB,
            Self::MaxRetainedBodyBytes => 32 * MIB,
            Self::MaxIconEntries => 32_768,
            Self::MaxAliasEntries => 32_768,
            Self::MaxTotalEntries => 65_536,
            Self::MaxAliasEdges => 32_768,
            Self::MaxAliasDepth => 64,
            Self::MaxAliasFanout => 1_024,
            Self::MaxBuildWorkUnits => 4_000_000,
            Self::MaxXmlElementsPerBody => 4_096,
            // Leave structural headroom beneath the WASM whole-SVG backend cap for wrappers and
            // diagram embedding. The final document validator remains authoritative.
            Self::MaxXmlDepthPerBody => crate::resources::MAX_PORTABLE_ICON_BODY_XML_DEPTH as u64,
            Self::MaxIdRewriteEditsPerBody => 16_384,
            Self::MaxRetainedXmlPlanBytes => 16 * MIB,
            Self::MaxCoordinateMagnitude => 1_000_000,
        }
    }

    #[must_use]
    pub const fn descriptor(self) -> &'static IconRegistryResourceLimitDescriptor {
        &ICON_REGISTRY_RESOURCE_LIMIT_DESCRIPTORS[self as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IconRegistryResourceLimitDescriptor {
    pub id: IconRegistryResourceLimitId,
    pub stable_id: &'static str,
    pub phase: &'static str,
    pub unit: &'static str,
    pub description: &'static str,
    pub default_value: u64,
    pub hard_maximum: u64,
    pub caller_configurable: bool,
}

const fn descriptor(
    id: IconRegistryResourceLimitId,
    phase: &'static str,
    unit: &'static str,
    description: &'static str,
) -> IconRegistryResourceLimitDescriptor {
    let value = id.fixed_value();
    IconRegistryResourceLimitDescriptor {
        id,
        stable_id: id.stable_id(),
        phase,
        unit,
        description,
        default_value: value,
        hard_maximum: value,
        caller_configurable: false,
    }
}

pub const ICON_REGISTRY_RESOURCE_LIMIT_DESCRIPTORS: &[IconRegistryResourceLimitDescriptor] = &[
    descriptor(
        IconRegistryResourceLimitId::MaxPacks,
        "icon_registry_input",
        "packs",
        "Maximum IconifyJSON packs admitted by one immutable registry",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxPackBytes,
        "icon_registry_input",
        "bytes",
        "Maximum encoded bytes admitted from one IconifyJSON pack",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxInputBytes,
        "icon_registry_input",
        "bytes",
        "Maximum aggregate encoded bytes admitted by one registry",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxJsonDepth,
        "icon_registry_parse",
        "levels",
        "Maximum JSON nesting depth inspected while ingesting packs",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxJsonMembers,
        "icon_registry_parse",
        "members",
        "Maximum aggregate JSON map members and sequence items inspected",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxJsonKeyBytes,
        "icon_registry_parse",
        "bytes",
        "Maximum decoded bytes in any IconifyJSON object key, including unknown extensions",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxPrefixBytes,
        "icon_registry_parse",
        "bytes",
        "Maximum admitted Iconify prefix or registration-name bytes",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxNameBytes,
        "icon_registry_parse",
        "bytes",
        "Maximum admitted Iconify icon or alias name bytes",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxBodyBytes,
        "icon_registry_xml",
        "bytes",
        "Maximum decoded SVG fragment bytes retained for one direct icon",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxRetainedBodyBytes,
        "icon_registry_retain",
        "bytes",
        "Maximum aggregate decoded direct-icon SVG body bytes retained",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxIconEntries,
        "icon_registry_parse",
        "entries",
        "Maximum direct icon entries admitted by one registry",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxAliasEntries,
        "icon_registry_alias",
        "entries",
        "Maximum alias entries admitted by one registry",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxTotalEntries,
        "icon_registry_alias",
        "entries",
        "Maximum resolved direct icon and alias entries published",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxAliasEdges,
        "icon_registry_alias",
        "edges",
        "Maximum parent edges inspected by alias resolution",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxAliasDepth,
        "icon_registry_alias",
        "levels",
        "Maximum resolved alias parent-chain depth",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxAliasFanout,
        "icon_registry_alias",
        "aliases",
        "Maximum aliases that may directly reference one parent",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxBuildWorkUnits,
        "icon_registry_build",
        "work_units",
        "Maximum deterministic parse, XML, and alias-resolution work",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxXmlElementsPerBody,
        "icon_registry_xml",
        "elements",
        "Maximum XML elements in one direct icon SVG fragment",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxXmlDepthPerBody,
        "icon_registry_xml",
        "levels",
        "Maximum XML element nesting depth in one direct icon fragment",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxIdRewriteEditsPerBody,
        "icon_registry_xml",
        "edits",
        "Maximum XML ID declaration and reference rewrites planned for one icon body",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxRetainedXmlPlanBytes,
        "icon_registry_retain",
        "bytes",
        "Maximum aggregate retained XML rewrite and defs-plan bytes",
    ),
    descriptor(
        IconRegistryResourceLimitId::MaxCoordinateMagnitude,
        "icon_registry_geometry",
        "coordinate_units",
        "Maximum absolute view-box coordinate or positive dimension",
    ),
];

pub const fn icon_registry_resource_limit_descriptors()
-> &'static [IconRegistryResourceLimitDescriptor] {
    ICON_REGISTRY_RESOURCE_LIMIT_DESCRIPTORS
}

#[derive(Debug, Clone, Copy)]
pub(super) struct IconRegistryBuildLimits {
    pub max_packs: usize,
    pub max_pack_bytes: usize,
    pub max_input_bytes: usize,
    pub max_json_depth: usize,
    pub max_json_members: usize,
    pub max_json_key_bytes: usize,
    pub max_prefix_bytes: usize,
    pub max_name_bytes: usize,
    pub max_body_bytes: usize,
    pub max_retained_body_bytes: usize,
    pub max_icon_entries: usize,
    pub max_alias_entries: usize,
    pub max_total_entries: usize,
    pub max_alias_edges: usize,
    pub max_alias_depth: usize,
    pub max_alias_fanout: usize,
    pub max_build_work_units: usize,
    pub max_xml_elements_per_body: usize,
    pub max_xml_depth_per_body: usize,
    pub max_id_rewrite_edits_per_body: usize,
    pub max_retained_xml_plan_bytes: usize,
    pub max_coordinate_magnitude: f64,
}

impl IconRegistryBuildLimits {
    pub const fn fixed() -> Self {
        Self {
            max_packs: IconRegistryResourceLimitId::MaxPacks.fixed_value() as usize,
            max_pack_bytes: IconRegistryResourceLimitId::MaxPackBytes.fixed_value() as usize,
            max_input_bytes: IconRegistryResourceLimitId::MaxInputBytes.fixed_value() as usize,
            max_json_depth: IconRegistryResourceLimitId::MaxJsonDepth.fixed_value() as usize,
            max_json_members: IconRegistryResourceLimitId::MaxJsonMembers.fixed_value() as usize,
            max_json_key_bytes: IconRegistryResourceLimitId::MaxJsonKeyBytes.fixed_value() as usize,
            max_prefix_bytes: IconRegistryResourceLimitId::MaxPrefixBytes.fixed_value() as usize,
            max_name_bytes: IconRegistryResourceLimitId::MaxNameBytes.fixed_value() as usize,
            max_body_bytes: IconRegistryResourceLimitId::MaxBodyBytes.fixed_value() as usize,
            max_retained_body_bytes: IconRegistryResourceLimitId::MaxRetainedBodyBytes.fixed_value()
                as usize,
            max_icon_entries: IconRegistryResourceLimitId::MaxIconEntries.fixed_value() as usize,
            max_alias_entries: IconRegistryResourceLimitId::MaxAliasEntries.fixed_value() as usize,
            max_total_entries: IconRegistryResourceLimitId::MaxTotalEntries.fixed_value() as usize,
            max_alias_edges: IconRegistryResourceLimitId::MaxAliasEdges.fixed_value() as usize,
            max_alias_depth: IconRegistryResourceLimitId::MaxAliasDepth.fixed_value() as usize,
            max_alias_fanout: IconRegistryResourceLimitId::MaxAliasFanout.fixed_value() as usize,
            max_build_work_units: IconRegistryResourceLimitId::MaxBuildWorkUnits.fixed_value()
                as usize,
            max_xml_elements_per_body: IconRegistryResourceLimitId::MaxXmlElementsPerBody
                .fixed_value() as usize,
            max_xml_depth_per_body: IconRegistryResourceLimitId::MaxXmlDepthPerBody.fixed_value()
                as usize,
            max_id_rewrite_edits_per_body: IconRegistryResourceLimitId::MaxIdRewriteEditsPerBody
                .fixed_value() as usize,
            max_retained_xml_plan_bytes: IconRegistryResourceLimitId::MaxRetainedXmlPlanBytes
                .fixed_value() as usize,
            max_coordinate_magnitude: IconRegistryResourceLimitId::MaxCoordinateMagnitude
                .fixed_value() as f64,
        }
    }
}
