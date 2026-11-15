pub(super) fn resolve_icon_key(icon_name: &str, fallback_prefix: Option<&str>) -> Option<String> {
    if icon_name.is_empty() || icon_name.trim() != icon_name {
        return None;
    }

    // Mirror Iconify's `stringToIcon(value, true, allowSimpleName)` ordering. A leading provider
    // is syntax only for this in-process registry: remove it first, then parse colon or dash forms.
    // Read at most four components so malformed input cannot allocate in proportion to the number
    // of separators before it is rejected.
    let mut parts = icon_name.splitn(4, ':');
    let first = parts.next()?;
    let second = parts.next();
    let third = parts.next();
    if parts.next().is_some() {
        return None;
    }

    let provider_present = icon_name.starts_with('@');
    let (qualified, simple_name) = if provider_present {
        match (second, third) {
            (Some(name), None) => (None, Some(name)),
            (Some(prefix), Some(name)) => (Some((prefix, name)), None),
            _ => return None,
        }
    } else {
        match (second, third) {
            (None, None) => (None, Some(first)),
            (Some(name), None) => (Some((first, name)), None),
            (Some(prefix), Some(name)) => (Some((prefix, name)), None),
            (None, Some(_)) => return None,
        }
    };

    if let Some((prefix, name)) = qualified {
        return (valid_identifier(prefix) && valid_identifier(name))
            .then(|| canonical_key(prefix, name));
    }

    let icon_name = simple_name?;

    // Iconify parses the dash shorthand before applying a simple-name fallback. In particular,
    // `aws-lambda` names the `aws:lambda` icon even in Architecture, while a truly simple name
    // such as `database` can resolve through `mermaid-architecture`.
    if let Some((prefix, name)) = icon_name.split_once('-')
        && valid_identifier(prefix)
        && valid_identifier(name)
    {
        return Some(canonical_key(prefix, name));
    }

    if provider_present {
        return None;
    }
    let fallback_prefix = fallback_prefix.filter(|prefix| valid_identifier(prefix))?;
    valid_identifier(icon_name).then(|| canonical_key(fallback_prefix, icon_name))
}

pub(super) fn valid_identifier(value: &str) -> bool {
    let mut segments = value.split('-');
    let Some(first) = segments.next() else {
        return false;
    };
    valid_identifier_segment(first) && segments.all(valid_identifier_segment)
}

fn valid_identifier_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub(super) fn canonical_key(prefix: &str, name: &str) -> String {
    let mut key = String::with_capacity(prefix.len() + name.len() + 1);
    key.push_str(prefix);
    key.push(':');
    key.push_str(name);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dash_shorthand_precedes_fallback_prefix() {
        assert_eq!(
            resolve_icon_key("aws-lambda", Some("mermaid-architecture")).as_deref(),
            Some("aws:lambda")
        );
        assert_eq!(
            resolve_icon_key("database", Some("mermaid-architecture")).as_deref(),
            Some("mermaid-architecture:database")
        );
    }

    #[test]
    fn provider_is_syntax_not_a_registry_namespace() {
        assert_eq!(
            resolve_icon_key("@cloud:logos:aws-lambda", None).as_deref(),
            Some("logos:aws-lambda")
        );
        assert_eq!(
            resolve_icon_key("cloud:logos:aws-lambda", None).as_deref(),
            Some("logos:aws-lambda")
        );
        assert_eq!(
            resolve_icon_key("@cloud:logos-aws", None).as_deref(),
            Some("logos:aws")
        );
        assert_eq!(
            resolve_icon_key("@cloud:logos-aws-lambda", None).as_deref(),
            Some("logos:aws-lambda")
        );
        assert_eq!(
            resolve_icon_key("@cloud:database", Some("mermaid-architecture")),
            None
        );
    }

    #[test]
    fn identifier_grammar_is_lowercase_ascii_segments() {
        for valid in ["a", "a1", "mermaid-architecture", "123"] {
            assert!(valid_identifier(valid), "{valid}");
        }
        for invalid in ["", "A", "a_b", "a.b", "-a", "a-", "a--b", "é"] {
            assert!(!valid_identifier(invalid), "{invalid}");
        }
    }

    #[test]
    fn excessive_colon_components_are_rejected_without_component_collection() {
        assert_eq!(resolve_icon_key("a:b:c:d", None), None);

        let many_components = "a:".repeat(64 * 1024);
        assert_eq!(resolve_icon_key(&many_components, None), None);
    }
}
