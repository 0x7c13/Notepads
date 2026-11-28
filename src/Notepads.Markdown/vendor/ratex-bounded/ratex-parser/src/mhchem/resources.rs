//! Builders for dynamic mhchem strings and JSON. Reserve before constructing;
//! owned strings/arrays are moved rather than serialized or deep-cloned.
use super::{MhchemResult, RuntimeCtx};
use crate::expansion_budget::Cost;
use serde_json::{Map, Value};

pub(super) trait IntoValue {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value>;
}
impl IntoValue for String {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        ctx.nodes(1)?;
        Ok(Value::String(self))
    }
}
impl IntoValue for &str {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        ctx.resources.charge(
            Some(Cost {
                nodes: 1,
                bytes: self.len(),
                tokens: 0,
            }),
            "mhchem string value",
        )?;
        Ok(Value::String(self.to_owned()))
    }
}
impl IntoValue for &String {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        self.as_str().into_value(ctx)
    }
}
impl IntoValue for Vec<Value> {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        ctx.nodes(1)?;
        Ok(Value::Array(self))
    }
}
impl IntoValue for Value {
    fn into_value(self, _: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        Ok(self)
    }
}
impl RuntimeCtx<'_> {
    pub(super) fn nodes(&self, nodes: usize) -> MhchemResult<()> {
        self.resources.charge(
            Some(Cost {
                nodes,
                ..Cost::default()
            }),
            "mhchem structure",
        )?;
        Ok(())
    }
    pub(super) fn value(&self, value: impl IntoValue) -> MhchemResult<Value> {
        value.into_value(self)
    }
    pub(super) fn object(&self) -> MhchemResult<Map<String, Value>> {
        self.nodes(1)?;
        Ok(Map::new())
    }
    pub(super) fn member(
        &self,
        map: &mut Map<String, Value>,
        key: &str,
        value: impl IntoValue,
    ) -> MhchemResult<()> {
        self.resources.charge(
            Some(Cost {
                nodes: 1,
                bytes: key.len(),
                tokens: 0,
            }),
            "mhchem object member",
        )?;
        let value = value.into_value(self)?;
        map.insert(key.to_owned(), value);
        Ok(())
    }
}

macro_rules! node {
    ($ctx:expr, {$($key:literal : $value:expr),* $(,)?}) => {{
        // Construct each value with its own preallocation check; never serialize
        // an already-built tree through serde_json::json!.
        let mut object = $ctx.object()?;
        $($ctx.member(&mut object, $key, $value)?;)*
        serde_json::Value::Object(object)
    }};
}
pub(super) use node;

impl IntoValue for std::borrow::Cow<'_, str> {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        match self {
            std::borrow::Cow::Borrowed(s) => s.into_value(ctx),
            std::borrow::Cow::Owned(s) => s.into_value(ctx),
        }
    }
}
impl IntoValue for &std::borrow::Cow<'_, str> {
    fn into_value(self, ctx: &RuntimeCtx<'_>) -> MhchemResult<Value> {
        self.as_ref().into_value(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion_budget::ExpansionBudget;
    use crate::stack_safety::{DepthBudget, MAX_INPUT_DEPTH};
    fn context(bytes: usize, nodes: usize) -> RuntimeCtx<'static> {
        RuntimeCtx {
            data: super::super::data(),
            depth_budget: DepthBudget::new(MAX_INPUT_DEPTH),
            resources: ExpansionBudget::new(Cost {
                tokens: 100_000,
                bytes,
                nodes,
            }),
        }
    }
    #[test]
    fn empty_values_and_members_are_checked_before_construction() {
        let ctx = context(0, 2);
        ctx.value("").unwrap();
        ctx.value(Vec::<Value>::new()).unwrap();
        let before = ctx.resources.used();
        assert!(ctx.value("").is_err());
        assert_eq!(ctx.resources.used(), before);
        let ctx = context(100, 1);
        let mut object = ctx.object().unwrap();
        let before = ctx.resources.used();
        assert!(ctx.member(&mut object, "key", "value").is_err());
        assert_eq!(ctx.resources.used(), before);
        assert!(object.is_empty());
    }
    #[test]
    fn bytes_and_structure_limits_reject_inside_mhchem() {
        for (bytes, nodes, input, expected) in [
            (32, 10000, "H".repeat(100), "text append"),
            (10000, 20, "H + O + C + N".into(), "mhchem"),
        ] {
            let ctx = context(bytes, nodes);
            let e = ctx.go(&input, "ce").unwrap_err();
            eprintln!("mhchem internal rejection: {e}");
            assert!(e.to_string().contains("resource limit"));
            assert!(e.to_string().contains(expected));
            assert!(ctx.resources.used().bytes <= bytes);
            assert!(ctx.resources.used().nodes <= nodes);
        }
    }
    #[test]
    fn texify_repeated_output_is_checked_before_format_allocation() {
        let ctx = context(1024, 100);
        let input = serde_json::json!({"type_":"frac", "p1":"x".repeat(300), "p2":"y"});
        let e = super::super::texify::go_with_budget(
            &[input],
            false,
            &ctx.depth_budget,
            &ctx.resources,
        )
        .unwrap_err();
        eprintln!("texify rejection: {e}");
        assert!(e.to_string().contains("formatted text"));
        assert!(ctx.resources.used().bytes < 1024);
    }
    #[test]
    fn patterns_borrow_remainders_and_limit_combined_capture() {
        let ctx = context(3, 100);
        let input = format!("{} rest", "1".repeat(100));
        let hit = super::super::patterns::match_pattern(ctx.data, "digits", &input, &ctx.resources)
            .unwrap()
            .unwrap();
        assert!(hit.remainder.as_ptr() >= input.as_ptr());
        assert_eq!(ctx.resources.used(), Cost::default());
        let e = super::super::patterns::match_pattern(
            ctx.data,
            r"\x{}{}",
            r"\underset{abc}{def}",
            &ctx.resources,
        )
        .err()
        .unwrap();
        assert!(e.to_string().contains("resource limit"));
        assert_eq!(ctx.resources.used(), Cost::default());
    }
    #[test]
    fn nested_mhchem_keeps_one_shared_budget() {
        let ctx = context(200, 10000);
        let e = super::super::chem_parse_str_with_resources(
            r"\ce{\ce{\ce{H2O}}}",
            "ce",
            ctx.depth_budget.clone(),
            ctx.resources.clone(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("resource limit"));
        assert!(ctx.resources.used().bytes <= 200);
        assert!(ctx.resources.used().bytes > 0);
    }
}
