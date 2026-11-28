pub(super) fn is_self_closing(tag: &str) -> bool {
    tag.trim_end().ends_with("/>")
}
