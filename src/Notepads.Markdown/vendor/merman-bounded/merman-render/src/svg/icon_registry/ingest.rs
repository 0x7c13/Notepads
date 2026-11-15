use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

use super::limits::{IconRegistryBuildLimits, IconRegistryResourceLimitId};
use super::xml::{ValidatedIconBody, validate_icon_body};
use super::{IconRegistryBuildError, IconRegistryBuildErrorKind};

const VALIDATION_SENTINEL: &str = "icon registry input validation failed";
const BODY_BYTES_PER_WORK_UNIT: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BuildUsage {
    pub(super) json_members: usize,
    pub(super) retained_body_bytes: usize,
    pub(super) retained_xml_plan_bytes: usize,
    pub(super) icons: usize,
    pub(super) aliases: usize,
    pub(super) entries: usize,
    pub(super) alias_edges: usize,
    pub(super) build_work_units: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct IconTransform {
    pub(super) rotate: u8,
    pub(super) h_flip: bool,
    pub(super) v_flip: bool,
}

impl IconTransform {
    fn then(self, child: Self) -> Self {
        Self {
            rotate: (self.rotate + child.rotate) % 4,
            h_flip: self.h_flip ^ child.h_flip,
            v_flip: self.v_flip ^ child.v_flip,
        }
    }
}

#[derive(Clone)]
pub(super) struct ResolvedIcon {
    pub(super) body: Arc<ValidatedIconBody>,
    pub(super) left: f64,
    pub(super) top: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) h_flip: bool,
    pub(super) v_flip: bool,
    pub(super) rotate: u8,
}

pub(super) struct ResolvedPack {
    pub(super) prefix: String,
    pub(super) registration_name: Option<String>,
    pub(super) icons: Vec<(String, ResolvedIcon)>,
}

pub(super) struct ParsedPack {
    pack_index: usize,
    limits: IconRegistryBuildLimits,
    prefix: String,
    registration_name: Option<String>,
    icons: Vec<NamedIcon>,
    icon_index: HashMap<String, usize>,
    aliases: Vec<AliasDefinition>,
    alias_index: HashMap<String, usize>,
}

impl ParsedPack {
    fn resolve_inner(
        self,
        global: &mut BuildUsage,
    ) -> Result<ResolvedPack, IconRegistryBuildError> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut colors = vec![Color::White; self.aliases.len()];
        let mut memo: Vec<Option<ResolvedIcon>> = vec![None; self.aliases.len()];
        let mut memo_depths = vec![0usize; self.aliases.len()];
        let mut path = Vec::new();

        for root in 0..self.aliases.len() {
            if colors[root] == Color::Black {
                continue;
            }

            path.clear();
            let mut current = root;
            let (mut resolved_parent, mut resolved_depth) = loop {
                charge_build_work(
                    global,
                    &self.limits,
                    self.pack_index,
                    1,
                    "alias graph traversal exceeds the build work budget",
                )?;

                match colors[current] {
                    Color::Black => {
                        break (
                            memo[current]
                                .as_ref()
                                .expect("black aliases are memoized")
                                .clone(),
                            memo_depths[current],
                        );
                    }
                    Color::Gray => {
                        return Err(build_error(
                            IconRegistryBuildErrorKind::AliasCycle,
                            self.pack_index,
                            "alias graph contains a cycle",
                        ));
                    }
                    Color::White => {}
                }

                colors[current] = Color::Gray;
                path.push(current);
                if path.len() > self.limits.max_alias_depth {
                    return Err(limit_error(
                        self.pack_index,
                        IconRegistryResourceLimitId::MaxAliasDepth,
                        path.len(),
                        self.limits.max_alias_depth,
                        "alias chain exceeds the configured depth",
                    ));
                }

                let parent = self.aliases[current].parent.as_str();
                if let Some(parent_index) = self.icon_index.get(parent) {
                    break (self.icons[*parent_index].icon.clone(), 0);
                }
                if let Some(parent_index) = self.alias_index.get(parent) {
                    current = *parent_index;
                    continue;
                }

                return Err(build_error(
                    IconRegistryBuildErrorKind::MissingAliasParent,
                    self.pack_index,
                    "alias parent does not exist in the pack",
                ));
            };

            while let Some(alias_index) = path.pop() {
                charge_build_work(
                    global,
                    &self.limits,
                    self.pack_index,
                    1,
                    "alias graph resolution exceeds the build work budget",
                )?;
                resolved_depth = resolved_depth.checked_add(1).ok_or_else(|| {
                    build_error(
                        IconRegistryBuildErrorKind::ArithmeticOverflow,
                        self.pack_index,
                        "alias depth accounting overflowed",
                    )
                })?;
                if resolved_depth > self.limits.max_alias_depth {
                    return Err(limit_error(
                        self.pack_index,
                        IconRegistryResourceLimitId::MaxAliasDepth,
                        resolved_depth,
                        self.limits.max_alias_depth,
                        "alias chain exceeds the configured depth",
                    ));
                }
                resolved_parent = self.aliases[alias_index].apply(resolved_parent);
                memo[alias_index] = Some(resolved_parent.clone());
                memo_depths[alias_index] = resolved_depth;
                colors[alias_index] = Color::Black;
            }
        }

        let capacity = self
            .icons
            .len()
            .checked_add(self.aliases.len())
            .ok_or_else(|| {
                build_error(
                    IconRegistryBuildErrorKind::ArithmeticOverflow,
                    self.pack_index,
                    "resolved pack entry count overflowed",
                )
            })?;
        let mut icons = Vec::with_capacity(capacity);
        icons.extend(self.icons.into_iter().map(|named| (named.name, named.icon)));
        icons.extend(self.aliases.into_iter().enumerate().map(|(index, alias)| {
            (
                alias.name,
                memo[index]
                    .take()
                    .expect("every alias is resolved before emission"),
            )
        }));

        Ok(ResolvedPack {
            prefix: self.prefix,
            registration_name: self.registration_name,
            icons,
        })
    }
}

pub(super) fn resolve_packs(
    packs: Vec<ParsedPack>,
    global: &mut BuildUsage,
) -> Result<Vec<ResolvedPack>, IconRegistryBuildError> {
    let mut working = *global;
    let mut resolved = Vec::with_capacity(packs.len());
    for pack in packs {
        let registration_name = pack.registration_name.clone();
        resolved.push(
            pack.resolve_inner(&mut working)
                .map_err(|error| error.with_registration_name(registration_name.as_deref()))?,
        );
    }
    *global = working;
    Ok(resolved)
}

pub(super) fn parse_pack(
    json: &[u8],
    registration_name: Option<&str>,
    pack_index: usize,
    limits: IconRegistryBuildLimits,
    global: &mut BuildUsage,
) -> Result<ParsedPack, IconRegistryBuildError> {
    if std::str::from_utf8(json).is_err() {
        return Err(build_error(
            IconRegistryBuildErrorKind::InvalidUtf8,
            pack_index,
            "Iconify JSON must be valid UTF-8",
        ));
    }

    let registration_name = registration_name.map(str::to_owned);

    let mut working = *global;
    let mut state = ParseState {
        pack_index,
        limits,
        ledger: &mut working,
        failure: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(json);
    let parsed = PackSeed {
        state: &mut state,
        registration_name,
    }
    .deserialize(&mut deserializer);

    let parsed = match parsed {
        Ok(pack) => pack,
        Err(error) => {
            if let Some(error) = state.failure.take() {
                return Err(error);
            }
            return Err(serde_error(pack_index, &error));
        }
    };
    if let Err(error) = deserializer.end() {
        return Err(serde_error(pack_index, &error));
    }

    drop(state);
    *global = working;
    Ok(parsed)
}

pub(super) fn validate_registration_name<'a>(
    registration_name: Option<&'a str>,
    pack_index: usize,
    limits: &IconRegistryBuildLimits,
) -> Result<Option<&'a str>, IconRegistryBuildError> {
    let Some(registration_name) = registration_name else {
        return Ok(None);
    };
    validate_identifier(
        registration_name,
        limits.max_prefix_bytes,
        IconRegistryResourceLimitId::MaxPrefixBytes,
        pack_index,
        "registration name is not a valid Iconify prefix",
    )?;
    Ok(Some(registration_name))
}

fn serde_error(pack_index: usize, error: &serde_json::Error) -> IconRegistryBuildError {
    let kind = match error.classify() {
        serde_json::error::Category::Data => IconRegistryBuildErrorKind::InvalidSchema,
        serde_json::error::Category::Io
        | serde_json::error::Category::Syntax
        | serde_json::error::Category::Eof => IconRegistryBuildErrorKind::InvalidJson,
    };
    build_error(kind, pack_index, "Iconify JSON could not be decoded")
}

fn build_error(
    kind: IconRegistryBuildErrorKind,
    pack_index: usize,
    message: &'static str,
) -> IconRegistryBuildError {
    IconRegistryBuildError::new(kind, Some(pack_index), message)
}

fn charge_build_work(
    ledger: &mut BuildUsage,
    limits: &IconRegistryBuildLimits,
    pack_index: usize,
    amount: usize,
    message: &'static str,
) -> Result<(), IconRegistryBuildError> {
    ledger.build_work_units = checked_limited_add(
        ledger.build_work_units,
        amount,
        limits.max_build_work_units,
        pack_index,
        IconRegistryResourceLimitId::MaxBuildWorkUnits,
        message,
    )?;
    Ok(())
}

fn checked_limited_add(
    current: usize,
    amount: usize,
    maximum: usize,
    pack_index: usize,
    limit: IconRegistryResourceLimitId,
    message: &'static str,
) -> Result<usize, IconRegistryBuildError> {
    let actual = current.checked_add(amount).ok_or_else(|| {
        build_error(
            IconRegistryBuildErrorKind::ArithmeticOverflow,
            pack_index,
            "icon registry resource accounting overflowed",
        )
    })?;
    if actual > maximum {
        return Err(limit_error(pack_index, limit, actual, maximum, message));
    }
    Ok(actual)
}

fn limit_error(
    pack_index: usize,
    limit: IconRegistryResourceLimitId,
    actual: usize,
    maximum: usize,
    message: &'static str,
) -> IconRegistryBuildError {
    build_error(
        IconRegistryBuildErrorKind::ResourceLimitExceeded,
        pack_index,
        message,
    )
    .with_limit(limit, actual, maximum)
}

fn validate_identifier(
    value: &str,
    maximum_bytes: usize,
    limit: IconRegistryResourceLimitId,
    pack_index: usize,
    message: &'static str,
) -> Result<(), IconRegistryBuildError> {
    if value.len() > maximum_bytes {
        return Err(limit_error(
            pack_index,
            limit,
            value.len(),
            maximum_bytes,
            message,
        ));
    }
    if !super::lookup::valid_identifier(value) {
        return Err(build_error(
            IconRegistryBuildErrorKind::InvalidIdentifier,
            pack_index,
            message,
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct GeometryOverrides {
    left: Option<f64>,
    top: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

impl GeometryOverrides {
    fn apply_to(self, left: f64, top: f64, width: f64, height: f64) -> (f64, f64, f64, f64) {
        (
            self.left.unwrap_or(left),
            self.top.unwrap_or(top),
            self.width.unwrap_or(width),
            self.height.unwrap_or(height),
        )
    }
}

struct RawIcon {
    name: String,
    body: Arc<ValidatedIconBody>,
    geometry: GeometryOverrides,
    transform: IconTransform,
}

struct NamedIcon {
    name: String,
    icon: ResolvedIcon,
}

struct AliasDefinition {
    name: String,
    parent: String,
    geometry: GeometryOverrides,
    transform: IconTransform,
}

impl AliasDefinition {
    fn apply(&self, parent: ResolvedIcon) -> ResolvedIcon {
        let (left, top, width, height) =
            self.geometry
                .apply_to(parent.left, parent.top, parent.width, parent.height);
        let transform = IconTransform {
            rotate: parent.rotate,
            h_flip: parent.h_flip,
            v_flip: parent.v_flip,
        }
        .then(self.transform);
        ResolvedIcon {
            body: parent.body,
            left,
            top,
            width,
            height,
            h_flip: transform.h_flip,
            v_flip: transform.v_flip,
            rotate: transform.rotate,
        }
    }
}

struct ParsedIcons {
    icons: Vec<RawIcon>,
    index: HashMap<String, usize>,
}

struct ParsedAliases {
    aliases: Vec<AliasDefinition>,
    index: HashMap<String, usize>,
}

struct ParseState<'a> {
    pack_index: usize,
    limits: IconRegistryBuildLimits,
    ledger: &'a mut BuildUsage,
    failure: Option<IconRegistryBuildError>,
}

impl ParseState<'_> {
    fn fail<E: de::Error>(&mut self, error: IconRegistryBuildError) -> E {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        E::custom(VALIDATION_SENTINEL)
    }

    fn schema_error<E: de::Error>(&mut self, message: &'static str) -> E {
        let error = build_error(
            IconRegistryBuildErrorKind::InvalidSchema,
            self.pack_index,
            message,
        );
        self.fail(error)
    }

    fn duplicate_key_error<E: de::Error>(&mut self) -> E {
        let error = build_error(
            IconRegistryBuildErrorKind::DuplicateJsonKey,
            self.pack_index,
            "JSON object contains a duplicate key",
        );
        self.fail(error)
    }

    fn allocation_error<E: de::Error>(&mut self, message: &'static str) -> E {
        let error = build_error(
            IconRegistryBuildErrorKind::AllocationFailed,
            self.pack_index,
            message,
        );
        self.fail(error)
    }

    fn check_container_depth<E: de::Error>(&mut self, depth: usize) -> Result<(), E> {
        if depth > self.limits.max_json_depth {
            return Err(self.fail(limit_error(
                self.pack_index,
                IconRegistryResourceLimitId::MaxJsonDepth,
                depth,
                self.limits.max_json_depth,
                "JSON nesting exceeds the configured depth",
            )));
        }
        Ok(())
    }

    fn charge_json_member<E: de::Error>(&mut self) -> Result<(), E> {
        self.ledger.json_members = match checked_limited_add(
            self.ledger.json_members,
            1,
            self.limits.max_json_members,
            self.pack_index,
            IconRegistryResourceLimitId::MaxJsonMembers,
            "JSON member count exceeds the configured budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        self.charge_work(1, "JSON decoding exceeds the build work budget")
    }

    fn charge_work<E: de::Error>(&mut self, amount: usize, message: &'static str) -> Result<(), E> {
        match charge_build_work(self.ledger, &self.limits, self.pack_index, amount, message) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.fail(error)),
        }
    }

    fn charge_icon<E: de::Error>(&mut self) -> Result<(), E> {
        self.ledger.icons = match checked_limited_add(
            self.ledger.icons,
            1,
            self.limits.max_icon_entries,
            self.pack_index,
            IconRegistryResourceLimitId::MaxIconEntries,
            "icon count exceeds the configured budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        self.charge_entry()
    }

    fn charge_alias<E: de::Error>(&mut self) -> Result<(), E> {
        self.ledger.aliases = match checked_limited_add(
            self.ledger.aliases,
            1,
            self.limits.max_alias_entries,
            self.pack_index,
            IconRegistryResourceLimitId::MaxAliasEntries,
            "alias count exceeds the configured budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        self.ledger.alias_edges = match checked_limited_add(
            self.ledger.alias_edges,
            1,
            self.limits.max_alias_edges,
            self.pack_index,
            IconRegistryResourceLimitId::MaxAliasEdges,
            "alias edge count exceeds the configured budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        self.charge_entry()
    }

    fn charge_entry<E: de::Error>(&mut self) -> Result<(), E> {
        self.ledger.entries = match checked_limited_add(
            self.ledger.entries,
            1,
            self.limits.max_total_entries,
            self.pack_index,
            IconRegistryResourceLimitId::MaxTotalEntries,
            "icon and alias count exceeds the configured budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        Ok(())
    }

    fn charge_retained_body<E: de::Error>(&mut self, bytes: usize) -> Result<(), E> {
        if bytes > self.limits.max_body_bytes {
            return Err(self.fail(limit_error(
                self.pack_index,
                IconRegistryResourceLimitId::MaxBodyBytes,
                bytes,
                self.limits.max_body_bytes,
                "icon body exceeds the configured byte limit",
            )));
        }
        self.ledger.retained_body_bytes = match checked_limited_add(
            self.ledger.retained_body_bytes,
            bytes,
            self.limits.max_retained_body_bytes,
            self.pack_index,
            IconRegistryResourceLimitId::MaxRetainedBodyBytes,
            "retained icon bodies exceed the configured byte budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        Ok(())
    }

    fn commit_xml_usage<E: de::Error>(
        &mut self,
        usage: super::xml::XmlAdmissionUsage,
    ) -> Result<(), E> {
        self.ledger.retained_xml_plan_bytes = match checked_limited_add(
            self.ledger.retained_xml_plan_bytes,
            usage.retained_plan_bytes,
            self.limits.max_retained_xml_plan_bytes,
            self.pack_index,
            IconRegistryResourceLimitId::MaxRetainedXmlPlanBytes,
            "retained XML rewrite plans exceed the registry byte budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        self.ledger.build_work_units = match checked_limited_add(
            self.ledger.build_work_units,
            usage.build_work_units,
            self.limits.max_build_work_units,
            self.pack_index,
            IconRegistryResourceLimitId::MaxBuildWorkUnits,
            "icon XML admission exceeds the build work budget",
        ) {
            Ok(value) => value,
            Err(error) => return Err(self.fail(error)),
        };
        Ok(())
    }

    fn validate_name<E: de::Error>(&mut self, name: &str, message: &'static str) -> Result<(), E> {
        validate_identifier(
            name,
            self.limits.max_name_bytes,
            IconRegistryResourceLimitId::MaxNameBytes,
            self.pack_index,
            message,
        )
        .map_err(|error| self.fail(error))
    }

    fn validate_coordinate<E: de::Error>(
        &mut self,
        value: f64,
        message: &'static str,
    ) -> Result<f64, E> {
        if !value.is_finite() {
            return Err(self.fail(build_error(
                IconRegistryBuildErrorKind::InvalidGeometry,
                self.pack_index,
                message,
            )));
        }
        if value.abs() > self.limits.max_coordinate_magnitude {
            let error = self.coordinate_limit_error(value, message);
            return Err(self.fail(error));
        }
        Ok(value)
    }

    fn validate_dimension<E: de::Error>(
        &mut self,
        value: f64,
        message: &'static str,
    ) -> Result<f64, E> {
        if !value.is_finite() || value <= 0.0 {
            return Err(self.fail(build_error(
                IconRegistryBuildErrorKind::InvalidGeometry,
                self.pack_index,
                message,
            )));
        }
        if value > self.limits.max_coordinate_magnitude {
            let error = self.coordinate_limit_error(value, message);
            return Err(self.fail(error));
        }
        Ok(value)
    }

    fn coordinate_limit_error(&self, value: f64, message: &'static str) -> IconRegistryBuildError {
        let actual = value.abs().ceil().min(u64::MAX as f64) as u64;
        let maximum = self.limits.max_coordinate_magnitude as u64;
        build_error(
            IconRegistryBuildErrorKind::ResourceLimitExceeded,
            self.pack_index,
            message,
        )
        .with_limit(
            IconRegistryResourceLimitId::MaxCoordinateMagnitude,
            actual,
            maximum,
        )
    }
}

struct PackSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    registration_name: Option<String>,
}

impl<'de> DeserializeSeed<'de> for PackSeed<'_, '_> {
    type Value = ParsedPack;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PackVisitor {
            state: self.state,
            registration_name: self.registration_name,
        })
    }
}

struct PackVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    registration_name: Option<String>,
}

impl<'de> Visitor<'de> for PackVisitor<'_, '_> {
    type Value = ParsedPack;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Iconify pack object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(1)?;
        let mut seen = HashSet::new();
        let mut prefix = None;
        let mut defaults = GeometryOverrides::default();
        let mut icons = None;
        let mut aliases = None;
        let registration_name = self.registration_name;

        while let Some(key) =
            next_unique_key(&mut map, &mut seen, self.state, JsonKeyPolicy::Generic)?
        {
            match key.as_ref() {
                "prefix" if registration_name.is_some() => map.next_value_seed(SkipValueSeed {
                    state: self.state,
                    depth: 2,
                })?,
                "prefix" => {
                    let maximum_bytes = self.state.limits.max_prefix_bytes;
                    prefix = Some(map.next_value_seed(BoundedStringSeed::identifier(
                        self.state,
                        maximum_bytes,
                        IconRegistryResourceLimitId::MaxPrefixBytes,
                        "pack prefix is not a valid Iconify identifier",
                    ))?);
                }
                "left" => {
                    let value = read_value(&mut map)?;
                    defaults.left = Some(
                        self.state
                            .validate_coordinate(value, "default left coordinate is invalid")?,
                    );
                }
                "top" => {
                    let value = read_value(&mut map)?;
                    defaults.top = Some(
                        self.state
                            .validate_coordinate(value, "default top coordinate is invalid")?,
                    );
                }
                "width" => {
                    let value = read_value(&mut map)?;
                    defaults.width = Some(
                        self.state
                            .validate_dimension(value, "default width is invalid")?,
                    );
                }
                "height" => {
                    let value = read_value(&mut map)?;
                    defaults.height = Some(
                        self.state
                            .validate_dimension(value, "default height is invalid")?,
                    );
                }
                "icons" => {
                    icons = Some(map.next_value_seed(IconMapSeed {
                        state: self.state,
                        depth: 2,
                    })?);
                }
                "aliases" => {
                    aliases = Some(map.next_value_seed(AliasMapSeed {
                        state: self.state,
                        depth: 2,
                    })?);
                }
                _ => map.next_value_seed(SkipValueSeed {
                    state: self.state,
                    depth: 2,
                })?,
            }
        }

        let prefix = match registration_name.as_ref() {
            Some(prefix) => prefix.clone(),
            None => prefix.ok_or_else(|| {
                self.state.schema_error::<A::Error>(
                    "Iconify pack is missing a prefix and no registration name was supplied",
                )
            })?,
        };
        let icons = icons.ok_or_else(|| {
            self.state
                .schema_error::<A::Error>("Iconify pack is missing the icons object")
        })?;
        let aliases = aliases.unwrap_or_else(|| ParsedAliases {
            aliases: Vec::new(),
            index: HashMap::new(),
        });

        for alias in &aliases.aliases {
            if icons.index.contains_key(alias.name.as_str()) {
                return Err(self.state.fail(build_error(
                    IconRegistryBuildErrorKind::DuplicateIcon,
                    self.state.pack_index,
                    "an icon and alias use the same name",
                )));
            }
        }

        let ParsedIcons {
            icons: raw_icons,
            index: icon_index,
        } = icons;
        let default_left = defaults.left.unwrap_or(0.0);
        let default_top = defaults.top.unwrap_or(0.0);
        let default_width = defaults.width.unwrap_or(16.0);
        let default_height = defaults.height.unwrap_or(16.0);
        let icons: Vec<NamedIcon> = raw_icons
            .into_iter()
            .map(|raw| {
                let (left, top, width, height) =
                    raw.geometry
                        .apply_to(default_left, default_top, default_width, default_height);
                NamedIcon {
                    name: raw.name,
                    icon: ResolvedIcon {
                        body: raw.body,
                        left,
                        top,
                        width,
                        height,
                        h_flip: raw.transform.h_flip,
                        v_flip: raw.transform.v_flip,
                        rotate: raw.transform.rotate,
                    },
                }
            })
            .collect();

        Ok(ParsedPack {
            pack_index: self.state.pack_index,
            limits: self.state.limits,
            prefix,
            registration_name,
            icons,
            icon_index,
            aliases: aliases.aliases,
            alias_index: aliases.index,
        })
    }
}

struct IconMapSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for IconMapSeed<'_, '_> {
    type Value = ParsedIcons;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(IconMapVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct IconMapVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> Visitor<'de> for IconMapVisitor<'_, '_> {
    type Value = ParsedIcons;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Iconify icons object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        let mut seen = HashSet::new();
        let mut icons = Vec::new();
        let mut index = HashMap::new();

        while let Some(key) = next_unique_key(
            &mut map,
            &mut seen,
            self.state,
            JsonKeyPolicy::Identifier {
                message: "icon name is not a valid Iconify identifier",
            },
        )? {
            let name = key.into_owned();
            self.state.charge_icon()?;
            let icon = map.next_value_seed(IconSeed {
                state: self.state,
                depth: self.depth + 1,
                name,
            })?;
            index.insert(icon.name.clone(), icons.len());
            icons.push(icon);
        }

        Ok(ParsedIcons { icons, index })
    }
}

struct IconSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
    name: String,
}

impl<'de> DeserializeSeed<'de> for IconSeed<'_, '_> {
    type Value = RawIcon;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(IconVisitor {
            state: self.state,
            depth: self.depth,
            name: self.name,
        })
    }
}

struct IconVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
    name: String,
}

impl<'de> Visitor<'de> for IconVisitor<'_, '_> {
    type Value = RawIcon;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Iconify icon object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        let mut seen = HashSet::new();
        let mut body = None;
        let mut geometry = GeometryOverrides::default();
        let mut transform = IconTransform::default();

        while let Some(key) =
            next_unique_key(&mut map, &mut seen, self.state, JsonKeyPolicy::Generic)?
        {
            match key.as_ref() {
                "body" => {
                    let maximum_bytes = self.state.limits.max_body_bytes;
                    body = Some(map.next_value_seed(BoundedStringSeed::icon_body(
                        self.state,
                        maximum_bytes,
                        IconRegistryResourceLimitId::MaxBodyBytes,
                        "icon body exceeds the configured byte limit",
                    ))?);
                }
                "left" => {
                    let value = read_value(&mut map)?;
                    geometry.left = Some(
                        self.state
                            .validate_coordinate(value, "icon left coordinate is invalid")?,
                    );
                }
                "top" => {
                    let value = read_value(&mut map)?;
                    geometry.top = Some(
                        self.state
                            .validate_coordinate(value, "icon top coordinate is invalid")?,
                    );
                }
                "width" => {
                    let value = read_value(&mut map)?;
                    geometry.width = Some(
                        self.state
                            .validate_dimension(value, "icon width is invalid")?,
                    );
                }
                "height" => {
                    let value = read_value(&mut map)?;
                    geometry.height = Some(
                        self.state
                            .validate_dimension(value, "icon height is invalid")?,
                    );
                }
                "rotate" => {
                    let rotate: QuarterTurns = read_value(&mut map)?;
                    transform.rotate = rotate.0;
                }
                "hFlip" => {
                    transform.h_flip = read_value(&mut map)?;
                }
                "vFlip" => {
                    transform.v_flip = read_value(&mut map)?;
                }
                _ => map.next_value_seed(SkipValueSeed {
                    state: self.state,
                    depth: self.depth + 1,
                })?,
            }
        }

        let body: String = body.ok_or_else(|| {
            self.state
                .schema_error::<A::Error>("icon definition is missing its body")
        })?;
        let (validated, xml_usage) = validate_icon_body(
            body,
            self.state.pack_index,
            &self.state.limits,
            self.state.ledger.retained_xml_plan_bytes,
            self.state.ledger.build_work_units,
        )
        .map_err(|error| self.state.fail(error))?;
        self.state.commit_xml_usage(xml_usage)?;

        Ok(RawIcon {
            name: self.name,
            body: validated,
            geometry,
            transform,
        })
    }
}

struct AliasMapSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for AliasMapSeed<'_, '_> {
    type Value = ParsedAliases;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(AliasMapVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct AliasMapVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> Visitor<'de> for AliasMapVisitor<'_, '_> {
    type Value = ParsedAliases;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Iconify aliases object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        let mut seen = HashSet::new();
        let mut aliases = Vec::new();
        let mut index = HashMap::new();
        let mut fanout: HashMap<String, usize> = HashMap::new();

        while let Some(key) = next_unique_key(
            &mut map,
            &mut seen,
            self.state,
            JsonKeyPolicy::Identifier {
                message: "alias name is not a valid Iconify identifier",
            },
        )? {
            let name = key.into_owned();
            self.state.charge_alias()?;
            let alias = map.next_value_seed(AliasSeed {
                state: self.state,
                depth: self.depth + 1,
                name,
            })?;

            let parent_fanout = fanout.entry(alias.parent.clone()).or_default();
            *parent_fanout = parent_fanout.checked_add(1).ok_or_else(|| {
                self.state.fail::<A::Error>(build_error(
                    IconRegistryBuildErrorKind::ArithmeticOverflow,
                    self.state.pack_index,
                    "alias fanout accounting overflowed",
                ))
            })?;
            if *parent_fanout > self.state.limits.max_alias_fanout {
                return Err(self.state.fail(limit_error(
                    self.state.pack_index,
                    IconRegistryResourceLimitId::MaxAliasFanout,
                    *parent_fanout,
                    self.state.limits.max_alias_fanout,
                    "alias parent fanout exceeds the configured budget",
                )));
            }

            index.insert(alias.name.clone(), aliases.len());
            aliases.push(alias);
        }

        Ok(ParsedAliases { aliases, index })
    }
}

struct AliasSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
    name: String,
}

impl<'de> DeserializeSeed<'de> for AliasSeed<'_, '_> {
    type Value = AliasDefinition;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(AliasVisitor {
            state: self.state,
            depth: self.depth,
            name: self.name,
        })
    }
}

struct AliasVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
    name: String,
}

impl<'de> Visitor<'de> for AliasVisitor<'_, '_> {
    type Value = AliasDefinition;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Iconify alias object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        let mut seen = HashSet::new();
        let mut parent = None;
        let mut geometry = GeometryOverrides::default();
        let mut transform = IconTransform::default();

        while let Some(key) =
            next_unique_key(&mut map, &mut seen, self.state, JsonKeyPolicy::Generic)?
        {
            match key.as_ref() {
                "parent" => {
                    let maximum_bytes = self.state.limits.max_name_bytes;
                    parent = Some(map.next_value_seed(BoundedStringSeed::identifier(
                        self.state,
                        maximum_bytes,
                        IconRegistryResourceLimitId::MaxNameBytes,
                        "alias parent is not a valid Iconify identifier",
                    ))?);
                }
                "left" => {
                    let value = read_value(&mut map)?;
                    geometry.left = Some(
                        self.state
                            .validate_coordinate(value, "alias left coordinate is invalid")?,
                    );
                }
                "top" => {
                    let value = read_value(&mut map)?;
                    geometry.top = Some(
                        self.state
                            .validate_coordinate(value, "alias top coordinate is invalid")?,
                    );
                }
                "width" => {
                    let value = read_value(&mut map)?;
                    geometry.width = Some(
                        self.state
                            .validate_dimension(value, "alias width is invalid")?,
                    );
                }
                "height" => {
                    let value = read_value(&mut map)?;
                    geometry.height = Some(
                        self.state
                            .validate_dimension(value, "alias height is invalid")?,
                    );
                }
                "rotate" => {
                    let rotate: QuarterTurns = read_value(&mut map)?;
                    transform.rotate = rotate.0;
                }
                "hFlip" => {
                    transform.h_flip = read_value(&mut map)?;
                }
                "vFlip" => {
                    transform.v_flip = read_value(&mut map)?;
                }
                _ => map.next_value_seed(SkipValueSeed {
                    state: self.state,
                    depth: self.depth + 1,
                })?,
            }
        }

        let parent: String = parent.ok_or_else(|| {
            self.state
                .schema_error::<A::Error>("alias definition is missing its parent")
        })?;
        Ok(AliasDefinition {
            name: self.name,
            parent,
            geometry,
            transform,
        })
    }
}

#[derive(Clone, Copy)]
struct QuarterTurns(u8);

impl<'de> Deserialize<'de> for QuarterTurns {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(QuarterTurnsVisitor)
    }
}

struct QuarterTurnsVisitor;

impl Visitor<'_> for QuarterTurnsVisitor {
    type Value = QuarterTurns;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer rotation")
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(QuarterTurns(value.rem_euclid(4) as u8))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(QuarterTurns((value % 4) as u8))
    }
}

fn read_value<'de, A, T>(map: &mut A) -> Result<T, A::Error>
where
    A: MapAccess<'de>,
    T: Deserialize<'de>,
{
    map.next_value()
}

struct BoundedStringSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    maximum_bytes: usize,
    limit: IconRegistryResourceLimitId,
    limit_message: &'static str,
    identifier_message: Option<&'static str>,
    accounting: BoundedStringAccounting,
}

#[derive(Clone, Copy)]
enum BoundedStringAccounting {
    None,
    IconBody,
}

impl<'a, 'ledger> BoundedStringSeed<'a, 'ledger> {
    fn icon_body(
        state: &'a mut ParseState<'ledger>,
        maximum_bytes: usize,
        limit: IconRegistryResourceLimitId,
        limit_message: &'static str,
    ) -> Self {
        Self {
            state,
            maximum_bytes,
            limit,
            limit_message,
            identifier_message: None,
            accounting: BoundedStringAccounting::IconBody,
        }
    }

    fn identifier(
        state: &'a mut ParseState<'ledger>,
        maximum_bytes: usize,
        limit: IconRegistryResourceLimitId,
        message: &'static str,
    ) -> Self {
        Self {
            state,
            maximum_bytes,
            limit,
            limit_message: message,
            identifier_message: Some(message),
            accounting: BoundedStringAccounting::None,
        }
    }
}

impl<'de> DeserializeSeed<'de> for BoundedStringSeed<'_, '_> {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(BoundedStringVisitor {
            state: self.state,
            maximum_bytes: self.maximum_bytes,
            limit: self.limit,
            limit_message: self.limit_message,
            identifier_message: self.identifier_message,
            accounting: self.accounting,
        })
    }
}

struct BoundedStringVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    maximum_bytes: usize,
    limit: IconRegistryResourceLimitId,
    limit_message: &'static str,
    identifier_message: Option<&'static str>,
    accounting: BoundedStringAccounting,
}

impl BoundedStringVisitor<'_, '_> {
    fn validate<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        if value.len() > self.maximum_bytes {
            return Err(self.state.fail(limit_error(
                self.state.pack_index,
                self.limit,
                value.len(),
                self.maximum_bytes,
                self.limit_message,
            )));
        }
        if let Some(message) = self.identifier_message
            && !super::lookup::valid_identifier(value)
        {
            return Err(self.state.fail(build_error(
                IconRegistryBuildErrorKind::InvalidIdentifier,
                self.state.pack_index,
                message,
            )));
        }
        if matches!(self.accounting, BoundedStringAccounting::IconBody) {
            self.state.charge_retained_body(value.len())?;
            self.state.charge_work(
                value.len().div_ceil(BODY_BYTES_PER_WORK_UNIT),
                "icon XML bytes exceed the build work budget",
            )?;
        }
        Ok(())
    }

    fn copy<E: de::Error>(&mut self, value: &str) -> Result<String, E> {
        self.validate(value)?;
        let mut owned = String::new();
        owned.try_reserve_exact(value.len()).map_err(|_| {
            self.state
                .allocation_error("bounded JSON string allocation failed")
        })?;
        owned.push_str(value);
        Ok(owned)
    }
}

impl<'de> Visitor<'de> for BoundedStringVisitor<'_, '_> {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded string")
    }

    fn visit_borrowed_str<E>(mut self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.copy(value)
    }

    fn visit_str<E>(mut self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.copy(value)
    }

    fn visit_string<E>(mut self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.validate(&value)?;
        Ok(value)
    }
}

struct JsonKeySeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    policy: JsonKeyPolicy,
}

#[derive(Clone, Copy)]
enum JsonKeyPolicy {
    Generic,
    Identifier { message: &'static str },
}

impl<'de> DeserializeSeed<'de> for JsonKeySeed<'_, '_> {
    type Value = Cow<'de, str>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(JsonKeyVisitor {
            state: self.state,
            policy: self.policy,
        })
    }
}

struct JsonKeyVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    policy: JsonKeyPolicy,
}

impl JsonKeyVisitor<'_, '_> {
    fn validate<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        if value.len() > self.state.limits.max_json_key_bytes {
            return Err(self.state.fail(limit_error(
                self.state.pack_index,
                IconRegistryResourceLimitId::MaxJsonKeyBytes,
                value.len(),
                self.state.limits.max_json_key_bytes,
                "JSON object key exceeds the fixed byte limit",
            )));
        }
        if let JsonKeyPolicy::Identifier { message } = self.policy {
            self.state.validate_name(value, message)?;
        }
        Ok(())
    }

    fn copy<E: de::Error>(&mut self, value: &str) -> Result<String, E> {
        self.validate(value)?;
        let mut owned = String::new();
        owned.try_reserve_exact(value.len()).map_err(|_| {
            self.state
                .allocation_error("JSON object key allocation failed")
        })?;
        owned.push_str(value);
        Ok(owned)
    }
}

impl<'de> Visitor<'de> for JsonKeyVisitor<'_, '_> {
    type Value = Cow<'de, str>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object key")
    }

    fn visit_borrowed_str<E>(mut self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.validate(value)?;
        Ok(Cow::Borrowed(value))
    }

    fn visit_str<E>(mut self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.copy(value).map(Cow::Owned)
    }

    fn visit_string<E>(mut self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.validate(&value)?;
        Ok(Cow::Owned(value))
    }
}

fn next_unique_key<'de, A>(
    map: &mut A,
    seen: &mut HashSet<String>,
    state: &mut ParseState<'_>,
    policy: JsonKeyPolicy,
) -> Result<Option<Cow<'de, str>>, A::Error>
where
    A: MapAccess<'de>,
{
    let Some(key) = map.next_key_seed(JsonKeySeed {
        state: &mut *state,
        policy,
    })?
    else {
        return Ok(None);
    };
    state.charge_json_member()?;
    let mut seen_key = String::new();
    seen_key
        .try_reserve_exact(key.len())
        .map_err(|_| state.allocation_error("duplicate-key tracking allocation failed"))?;
    seen_key.push_str(key.as_ref());
    seen.try_reserve(1)
        .map_err(|_| state.allocation_error("duplicate-key set allocation failed"))?;
    if !seen.insert(seen_key) {
        return Err(state.duplicate_key_error());
    }
    Ok(Some(key))
}

struct SkipValueSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for SkipValueSeed<'_, '_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SkipValueVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct SkipValueVisitor<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> Visitor<'de> for SkipValueVisitor<'_, '_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any bounded JSON value")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_char<E>(self, _value: char) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_borrowed_str<E>(self, _value: &'de str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_bytes<E>(self, _value: &[u8]) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_borrowed_bytes<E>(self, _value: &'de [u8]) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_byte_buf<E>(self, _value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        SkipValueSeed {
            state: self.state,
            depth: self.depth,
        }
        .deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        SkipValueSeed {
            state: self.state,
            depth: self.depth,
        }
        .deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        while sequence
            .next_element_seed(SkipSequenceElementSeed {
                state: self.state,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.state.check_container_depth(self.depth)?;
        let mut seen = HashSet::new();
        while next_unique_key(&mut map, &mut seen, self.state, JsonKeyPolicy::Generic)?.is_some() {
            map.next_value_seed(SkipValueSeed {
                state: self.state,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

struct SkipSequenceElementSeed<'a, 'ledger> {
    state: &'a mut ParseState<'ledger>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for SkipSequenceElementSeed<'_, '_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        self.state.charge_json_member()?;
        SkipValueSeed {
            state: self.state,
            depth: self.depth,
        }
        .deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "<path/>";

    struct ParsedRegistry {
        icons: HashMap<String, ResolvedIcon>,
        usage: BuildUsage,
    }

    fn limits() -> IconRegistryBuildLimits {
        IconRegistryBuildLimits::fixed()
    }

    fn parse(json: &str) -> Result<ParsedRegistry, IconRegistryBuildError> {
        parse_and_resolve_pack(json.as_bytes(), None, 0, limits())
    }

    fn parse_and_resolve_pack(
        json: &[u8],
        registration_name: Option<&str>,
        pack_index: usize,
        limits: IconRegistryBuildLimits,
    ) -> Result<ParsedRegistry, IconRegistryBuildError> {
        let mut usage = BuildUsage::default();
        let parsed = parse_pack(json, registration_name, pack_index, limits, &mut usage)?;
        let mut resolved = resolve_packs(vec![parsed], &mut usage)?;
        let pack = resolved.pop().expect("one parsed pack resolves once");
        let icons = pack
            .icons
            .into_iter()
            .map(|(name, icon)| (format!("{}:{name}", pack.prefix), icon))
            .collect();
        Ok(ParsedRegistry { icons, usage })
    }

    fn resolved<'a>(registry: &'a ParsedRegistry, key: &str) -> &'a ResolvedIcon {
        registry.icons.get(key).expect("resolved icon")
    }

    fn escaped_ascii_a(count: usize) -> String {
        "\\u0061".repeat(count)
    }

    fn error_of<T>(result: Result<T, IconRegistryBuildError>) -> IconRegistryBuildError {
        match result {
            Ok(_) => panic!("expected icon registry build failure"),
            Err(error) => error,
        }
    }

    fn assert_limit_error(
        error: &IconRegistryBuildError,
        id: IconRegistryResourceLimitId,
        actual: usize,
        maximum: usize,
    ) {
        assert_eq!(
            error.kind(),
            IconRegistryBuildErrorKind::ResourceLimitExceeded
        );
        assert_eq!(error.limit_id(), Some(id.stable_id()));
        assert_eq!(error.actual(), u64::try_from(actual).ok());
        assert_eq!(error.maximum(), u64::try_from(maximum).ok());
    }

    #[test]
    fn parses_defaults_after_icons_and_applies_geometry_overrides() {
        let registry = parse(
            r#"{
                "prefix":"test",
                "icons":{"base":{"body":"<path/>","width":12}},
                "left":-2,"top":3,"width":24,"height":32
            }"#,
        )
        .unwrap();

        let icon = resolved(&registry, "test:base");
        assert_eq!((icon.left, icon.top), (-2.0, 3.0));
        assert_eq!((icon.width, icon.height), (12.0, 32.0));
        assert_eq!(registry.usage.icons, 1);
        assert_eq!(registry.usage.entries, 1);
        assert_eq!(registry.usage.retained_body_bytes, BODY.len());
    }

    #[test]
    fn registration_name_overrides_and_skips_the_json_prefix_value() {
        let registry = parse_and_resolve_pack(
            br#"{
                "prefix":{"invalid":{"but":"bounded"}},
                "icons":{"base":{"body":"<path/>"}}
            }"#,
            Some("registered-pack"),
            0,
            limits(),
        )
        .unwrap();

        assert!(registry.icons.contains_key("registered-pack:base"));
    }

    #[test]
    fn rejects_duplicate_keys_before_schema_projection() {
        let duplicate_top =
            r#"{"prefix":"test","prefix":"other","icons":{"base":{"body":"<path/>"}}}"#;
        let duplicate_icon_field = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>","\u0062ody":"<g/>"}}
        }"#;
        let duplicate_icon_name = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"},"base":{"body":"<g/>"}}
        }"#;
        let duplicate_escaped_icon_name = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"},"\u0062ase":{"body":"<g/>"}}
        }"#;
        let duplicate_unknown_nested = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"}},
            "metadata":{"nested":{"same":1,"same":2}}
        }"#;

        for json in [
            duplicate_top,
            duplicate_icon_field,
            duplicate_icon_name,
            duplicate_escaped_icon_name,
            duplicate_unknown_nested,
        ] {
            assert!(parse(json).is_err(), "duplicate key was accepted: {json}");
        }
    }

    #[test]
    fn decoded_json_key_limit_is_exact_for_borrowed_and_escaped_keys() {
        let mut bounded = limits();
        bounded.max_json_key_bytes = 8;

        for key in ["a".repeat(8), escaped_ascii_a(8)] {
            let json = format!(
                r#"{{"prefix":"test","icons":{{"base":{{"body":"<path/>"}}}},"{key}":null}}"#
            );
            assert!(
                parse_and_resolve_pack(json.as_bytes(), None, 0, bounded).is_ok(),
                "decoded exact-limit key was rejected"
            );
        }

        for key in ["a".repeat(9), escaped_ascii_a(9)] {
            let json = format!(
                r#"{{"prefix":"test","icons":{{"base":{{"body":"<path/>"}}}},"{key}":null}}"#
            );
            let error = error_of(parse_and_resolve_pack(json.as_bytes(), None, 0, bounded));
            assert_limit_error(&error, IconRegistryResourceLimitId::MaxJsonKeyBytes, 9, 8);
        }
    }

    #[test]
    fn decoded_identifier_limits_are_exact_before_retained_copies() {
        let mut prefix_limits = limits();
        prefix_limits.max_prefix_bytes = 4;
        for prefix in ["a".repeat(4), escaped_ascii_a(4)] {
            let json = format!(r#"{{"prefix":"{prefix}","icons":{{}}}}"#);
            assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, prefix_limits).is_ok());
        }
        for prefix in ["a".repeat(5), escaped_ascii_a(5)] {
            let json = format!(r#"{{"prefix":"{prefix}","icons":{{}}}}"#);
            let error = error_of(parse_and_resolve_pack(
                json.as_bytes(),
                None,
                0,
                prefix_limits,
            ));
            assert_limit_error(&error, IconRegistryResourceLimitId::MaxPrefixBytes, 5, 4);
        }

        let mut name_limits = limits();
        name_limits.max_name_bytes = 4;
        for name in ["a".repeat(4), escaped_ascii_a(4)] {
            let json = format!(r#"{{"prefix":"test","icons":{{"{name}":{{"body":"<path/>"}}}}}}"#);
            assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, name_limits).is_ok());
        }
        for name in ["a".repeat(5), escaped_ascii_a(5)] {
            let json = format!(r#"{{"prefix":"test","icons":{{"{name}":{{"body":"<path/>"}}}}}}"#);
            let error = error_of(parse_and_resolve_pack(
                json.as_bytes(),
                None,
                0,
                name_limits,
            ));
            assert_limit_error(&error, IconRegistryResourceLimitId::MaxNameBytes, 5, 4);
        }

        for alias_name in ["a".repeat(4), escaped_ascii_a(4)] {
            let json = format!(
                r#"{{"prefix":"test","icons":{{"base":{{"body":"<path/>"}}}},"aliases":{{"{alias_name}":{{"parent":"base"}}}}}}"#
            );
            assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, name_limits).is_ok());
        }
        for alias_name in ["a".repeat(5), escaped_ascii_a(5)] {
            let json = format!(
                r#"{{"prefix":"test","icons":{{"base":{{"body":"<path/>"}}}},"aliases":{{"{alias_name}":{{"parent":"base"}}}}}}"#
            );
            let error = error_of(parse_and_resolve_pack(
                json.as_bytes(),
                None,
                0,
                name_limits,
            ));
            assert_limit_error(&error, IconRegistryResourceLimitId::MaxNameBytes, 5, 4);
        }

        let exact_parent = escaped_ascii_a(4);
        let exact_alias = format!(
            r#"{{"prefix":"test","icons":{{"aaaa":{{"body":"<path/>"}}}},"aliases":{{"b":{{"parent":"{exact_parent}"}}}}}}"#
        );
        assert!(parse_and_resolve_pack(exact_alias.as_bytes(), None, 0, name_limits).is_ok());

        let over_parent = escaped_ascii_a(5);
        let over_alias = format!(
            r#"{{"prefix":"test","icons":{{"aaaa":{{"body":"<path/>"}}}},"aliases":{{"b":{{"parent":"{over_parent}"}}}}}}"#
        );
        let error = error_of(parse_and_resolve_pack(
            over_alias.as_bytes(),
            None,
            0,
            name_limits,
        ));
        assert_limit_error(&error, IconRegistryResourceLimitId::MaxNameBytes, 5, 4);
    }

    #[test]
    fn decoded_body_limit_is_exact_and_charged_before_retention() {
        let mut bounded = limits();
        bounded.max_body_bytes = 4;
        bounded.max_retained_body_bytes = 4;

        for body in [r#"<g/>"#.to_string(), r#"<\u0067/>"#.to_string()] {
            let json = format!(r#"{{"prefix":"test","icons":{{"a":{{"body":"{body}"}}}}}}"#);
            let registry = parse_and_resolve_pack(json.as_bytes(), None, 0, bounded)
                .expect("decoded exact-limit body must be admitted");
            assert_eq!(registry.usage.retained_body_bytes, 4);
        }

        let json = r#"{"prefix":"test","icons":{"a":{"body":"<g />"}}}"#;
        let error = error_of(parse_and_resolve_pack(json.as_bytes(), None, 0, bounded));
        assert_limit_error(&error, IconRegistryResourceLimitId::MaxBodyBytes, 5, 4);
    }

    #[test]
    fn identifier_and_schema_failures_are_classified_without_input_echo() {
        for json in [
            r#"{"prefix":"té","icons":{}}"#,
            r#"{"prefix":"test","icons":{"é":{"body":"<path/>"}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>"}},"aliases":{"é":{"parent":"base"}}}"#,
        ] {
            let error = error_of(parse(json));
            assert_eq!(error.kind(), IconRegistryBuildErrorKind::InvalidIdentifier);
            assert!(!error.to_string().contains(json));
        }

        for json in [
            r#"{"prefix":1,"icons":{}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":1}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>"}},"aliases":{"alias":{"parent":1}}}"#,
        ] {
            let error = error_of(parse(json));
            assert_eq!(error.kind(), IconRegistryBuildErrorKind::InvalidSchema);
            assert!(!error.to_string().contains(json));
        }
    }

    #[test]
    fn enforces_unknown_value_depth_at_the_exact_boundary() {
        let exact = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"}},
            "metadata":{"a":{}}
        }"#;
        let over = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"}},
            "metadata":{"a":{"b":{}}}
        }"#;
        let mut bounded = limits();
        bounded.max_json_depth = 3;

        assert!(
            parse_and_resolve_pack(exact.as_bytes(), None, 0, bounded).is_ok(),
            "the exact depth limit must be accepted"
        );
        assert!(
            parse_and_resolve_pack(over.as_bytes(), None, 0, bounded).is_err(),
            "limit + 1 must be rejected"
        );
    }

    #[test]
    fn counts_map_members_and_unknown_sequence_elements_exactly() {
        let json = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"}},
            "metadata":[0,1]
        }"#;
        let mut exact = limits();
        exact.max_json_members = 7;
        assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, exact).is_ok());

        let mut over = exact;
        over.max_json_members = 6;
        assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, over).is_err());
    }

    #[test]
    fn validates_strict_ascii_iconify_identifiers() {
        let valid = r#"{
            "prefix":"pack-1",
            "icons":{"a1-b2":{"body":"<path/>"}}
        }"#;
        assert!(parse(valid).is_ok());

        for invalid in [
            "Upper",
            "under_score",
            "-leading",
            "trailing-",
            "two--parts",
            "",
        ] {
            let json =
                format!(r#"{{"prefix":"test","icons":{{"{invalid}":{{"body":"<path/>"}}}}}}"#);
            assert!(
                parse(&json).is_err(),
                "invalid identifier was accepted: {invalid}"
            );
        }

        let long_name = "a".repeat(limits().max_name_bytes + 1);
        let json = format!(r#"{{"prefix":"test","icons":{{"{long_name}":{{"body":"<path/>"}}}}}}"#);
        assert!(parse(&json).is_err());

        let long_prefix = "a".repeat(limits().max_prefix_bytes + 1);
        let json = format!(r#"{{"prefix":"{long_prefix}","icons":{{}}}}"#);
        assert!(parse(&json).is_err());
    }

    #[test]
    fn validates_geometry_and_integer_rotations() {
        for json in [
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","width":0}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","height":-1}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","rotate":1.0}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","hFlip":1}}}"#,
        ] {
            assert!(
                parse(json).is_err(),
                "invalid geometry was accepted: {json}"
            );
        }

        let registry = parse(
            r#"{
                "prefix":"test",
                "icons":{"base":{"body":"<path/>","rotate":-1}}
            }"#,
        )
        .unwrap();
        assert_eq!(resolved(&registry, "test:base").rotate, 3);

        let exact = parse(
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","left":1000000,"width":1000000}}}"#,
        )
        .expect("the exact coordinate magnitude must be accepted");
        assert_eq!(resolved(&exact, "test:base").left, 1_000_000.0);
        assert_eq!(resolved(&exact, "test:base").width, 1_000_000.0);

        for json in [
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","left":1000001}}}"#,
            r#"{"prefix":"test","icons":{"base":{"body":"<path/>","width":1000001}}}"#,
        ] {
            let error = match parse(json) {
                Ok(_) => panic!("limit + 1 must be rejected"),
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                IconRegistryBuildErrorKind::ResourceLimitExceeded
            );
            assert_eq!(
                error.limit_id(),
                Some(IconRegistryResourceLimitId::MaxCoordinateMagnitude.stable_id())
            );
            assert_eq!(error.actual(), Some(1_000_001));
            assert_eq!(error.maximum(), Some(1_000_000));
        }
    }

    #[test]
    fn resolves_alias_chains_with_geometry_inheritance() {
        let registry = parse(
            r#"{
                "prefix":"test","width":24,"height":32,
                "icons":{"base":{"body":"<path/>","left":1}},
                "aliases":{
                    "middle":{"parent":"base","top":2,"width":20},
                    "leaf":{"parent":"middle","height":12}
                }
            }"#,
        )
        .unwrap();

        let leaf = resolved(&registry, "test:leaf");
        assert_eq!((leaf.left, leaf.top), (1.0, 2.0));
        assert_eq!((leaf.width, leaf.height), (20.0, 12.0));
    }

    #[test]
    fn rejects_alias_cycles_missing_parents_and_name_conflicts() {
        let cycle = r#"{
            "prefix":"test","icons":{},
            "aliases":{"a":{"parent":"b"},"b":{"parent":"a"}}
        }"#;
        let missing = r#"{
            "prefix":"test","icons":{},
            "aliases":{"a":{"parent":"missing"}}
        }"#;
        let conflict = r#"{
            "prefix":"test","icons":{"same":{"body":"<path/>"}},
            "aliases":{"same":{"parent":"same"}}
        }"#;

        assert!(parse(cycle).is_err());
        assert!(parse(missing).is_err());
        assert!(parse(conflict).is_err());
    }

    #[test]
    fn enforces_alias_depth_and_fanout_at_exact_boundaries() {
        let depth_exact = r#"{
            "prefix":"test","icons":{"base":{"body":"<path/>"}},
            "aliases":{"a":{"parent":"base"},"b":{"parent":"a"}}
        }"#;
        let depth_over = r#"{
            "prefix":"test","icons":{"base":{"body":"<path/>"}},
            "aliases":{
                "a":{"parent":"base"},
                "b":{"parent":"a"},
                "c":{"parent":"b"}
            }
        }"#;
        let mut depth_limits = limits();
        depth_limits.max_alias_depth = 2;
        assert!(parse_and_resolve_pack(depth_exact.as_bytes(), None, 0, depth_limits).is_ok());
        assert!(parse_and_resolve_pack(depth_over.as_bytes(), None, 0, depth_limits).is_err());

        let fanout_exact = r#"{
            "prefix":"test","icons":{"base":{"body":"<path/>"}},
            "aliases":{"a":{"parent":"base"},"b":{"parent":"base"}}
        }"#;
        let fanout_over = r#"{
            "prefix":"test","icons":{"base":{"body":"<path/>"}},
            "aliases":{
                "a":{"parent":"base"},
                "b":{"parent":"base"},
                "c":{"parent":"base"}
            }
        }"#;
        let mut fanout_limits = limits();
        fanout_limits.max_alias_fanout = 2;
        assert!(parse_and_resolve_pack(fanout_exact.as_bytes(), None, 0, fanout_limits).is_ok());
        assert!(parse_and_resolve_pack(fanout_over.as_bytes(), None, 0, fanout_limits).is_err());
    }

    #[test]
    fn composes_alias_transforms_and_shares_the_validated_body() {
        let registry = parse(
            r#"{
                "prefix":"test",
                "icons":{
                    "base":{"body":"<path/>","rotate":-1,"hFlip":true}
                },
                "aliases":{
                    "alias":{"parent":"base","rotate":6,"hFlip":true,"vFlip":true}
                }
            }"#,
        )
        .unwrap();

        let base = resolved(&registry, "test:base");
        let alias = resolved(&registry, "test:alias");
        assert_eq!(alias.rotate, 1);
        assert!(!alias.h_flip);
        assert!(alias.v_flip);
        assert!(Arc::ptr_eq(&base.body, &alias.body));
        assert_eq!(registry.usage.retained_body_bytes, BODY.len());
    }

    #[test]
    fn body_and_retained_body_limits_are_exact() {
        let json = r#"{"prefix":"test","icons":{"base":{"body":"<path/>"}}}"#;
        let mut exact = limits();
        exact.max_body_bytes = BODY.len();
        exact.max_retained_body_bytes = BODY.len();
        assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, exact).is_ok());

        let mut body_over = exact;
        body_over.max_body_bytes = BODY.len() - 1;
        assert!(parse_and_resolve_pack(json.as_bytes(), None, 0, body_over).is_err());

        let two_icons = r#"{
            "prefix":"test",
            "icons":{"a":{"body":"<path/>"},"b":{"body":"<path/>"}}
        }"#;
        assert!(
            parse_and_resolve_pack(two_icons.as_bytes(), None, 0, exact).is_err(),
            "retained bodies are counted once per direct icon"
        );
    }

    #[test]
    fn enforces_entry_edge_and_work_budgets_at_exact_boundaries() {
        let one_icon_one_alias = r#"{
            "prefix":"test",
            "icons":{"base":{"body":"<path/>"}},
            "aliases":{"alias":{"parent":"base"}}
        }"#;

        let mut exact = limits();
        exact.max_icon_entries = 1;
        exact.max_alias_entries = 1;
        exact.max_total_entries = 2;
        exact.max_alias_edges = 1;
        let baseline =
            parse_and_resolve_pack(one_icon_one_alias.as_bytes(), None, 0, exact).unwrap();

        let mut over = exact;
        over.max_total_entries = 1;
        assert!(parse_and_resolve_pack(one_icon_one_alias.as_bytes(), None, 0, over).is_err());
        over = exact;
        over.max_alias_edges = 0;
        assert!(parse_and_resolve_pack(one_icon_one_alias.as_bytes(), None, 0, over).is_err());

        let mut exact_work = exact;
        exact_work.max_build_work_units = baseline.usage.build_work_units;
        assert!(parse_and_resolve_pack(one_icon_one_alias.as_bytes(), None, 0, exact_work).is_ok());
        let mut over_work = exact_work;
        over_work.max_build_work_units -= 1;
        assert!(parse_and_resolve_pack(one_icon_one_alias.as_bytes(), None, 0, over_work).is_err());
    }

    #[test]
    fn shared_ledger_enforces_limits_across_multiple_pack_parses() {
        let json = r#"{"prefix":"test","icons":{"base":{"body":"<path/>"}}}"#;
        let mut bounded = limits();
        bounded.max_icon_entries = 1;
        bounded.max_total_entries = 1;
        let mut ledger = BuildUsage::default();

        let first = parse_pack(json.as_bytes(), None, 0, bounded, &mut ledger).unwrap();
        assert_eq!(ledger.icons, 1);
        let after_first = ledger;
        assert!(parse_pack(json.as_bytes(), None, 1, bounded, &mut ledger).is_err());
        assert_eq!(ledger, after_first);
        drop(first);
    }

    #[test]
    fn retained_body_budget_is_transactional_across_packs() {
        let first_json = r#"{"prefix":"one","icons":{"base":{"body":"<path/>"}}}"#;
        let second_json = r#"{"prefix":"two","icons":{"base":{"body":"<path/>"}}}"#;
        let mut bounded = limits();
        bounded.max_retained_body_bytes = BODY.len();
        let mut ledger = BuildUsage::default();

        let first = parse_pack(first_json.as_bytes(), None, 0, bounded, &mut ledger).unwrap();
        let after_first = ledger;
        let error = error_of(parse_pack(
            second_json.as_bytes(),
            None,
            1,
            bounded,
            &mut ledger,
        ));
        assert_limit_error(
            &error,
            IconRegistryResourceLimitId::MaxRetainedBodyBytes,
            BODY.len() * 2,
            BODY.len(),
        );
        assert_eq!(
            ledger, after_first,
            "failed packs must roll back their usage"
        );
        drop(first);
    }

    #[test]
    fn parse_and_resolution_failures_do_not_partially_mutate_the_ledger() {
        let mut ledger = BuildUsage::default();
        let invalid = r#"{"prefix":"test","icons":{"base":{}}}"#;
        assert!(parse_pack(invalid.as_bytes(), None, 0, limits(), &mut ledger).is_err());
        assert_eq!(ledger, BuildUsage::default());

        let unresolved = r#"{
            "prefix":"test","icons":{},
            "aliases":{"a":{"parent":"missing"}}
        }"#;
        let parsed = parse_pack(unresolved.as_bytes(), None, 0, limits(), &mut ledger).unwrap();
        let after_parse = ledger;
        assert!(resolve_packs(vec![parsed], &mut ledger).is_err());
        assert_eq!(ledger, after_parse);
    }

    #[test]
    fn rejects_invalid_utf8_malformed_json_and_non_object_roots() {
        assert!(
            parse_and_resolve_pack(&[0xff], None, 0, limits()).is_err(),
            "invalid UTF-8 must be rejected before JSON decoding"
        );
        assert!(parse_and_resolve_pack(br#"{"prefix":}"#, None, 0, limits()).is_err());
        assert!(parse_and_resolve_pack(br#"[]"#, None, 0, limits()).is_err());
    }
}
