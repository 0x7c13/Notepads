use std::{
    error::Error,
    fmt::Display,
    io::{Error as IoError, ErrorKind},
};

use syntect::{
    parsing::{ParseScopeError, ParseSyntaxError},
    LoadingError,
};

#[test]
fn loading_error_bad_path_display() {
    assert_display(LoadingError::BadPath, "Invalid path");
}

#[test]
fn loading_error_parse_syntax_display() {
    assert_display(
        LoadingError::ParseSyntax(
            ParseSyntaxError::MissingMandatoryKey("main"),
            String::from("file.sublime-syntax"),
        ),
        "file.sublime-syntax: Missing mandatory key in YAML file: main",
    );
}

#[test]
fn loading_error_io_source() {
    let io_error_source = IoError::new(ErrorKind::Other, "this is an error string");
    assert_display(
        LoadingError::Io(io_error_source).source().unwrap(),
        "this is an error string",
    );
}

#[test]
fn parse_syntax_error_missing_mandatory_key_display() {
    assert_display(
        ParseSyntaxError::MissingMandatoryKey("mandatory_key"),
        "Missing mandatory key in YAML file: mandatory_key",
    );
}

#[test]
fn parse_syntax_error_regex_compile_error_display() {
    assert_display(
        ParseSyntaxError::RegexCompileError("[a-Z]".to_owned(), LoadingError::BadPath.into()),
        "Error while compiling regex '[a-Z]': Invalid path",
    );
}

#[test]
fn parse_scope_error_display() {
    assert_display(
        ParseScopeError::TooLong,
        "Too long scope. Scopes can be at most 8 atoms long.",
    )
}

#[test]
fn parse_syntax_error_regex_compile_error_source() {
    let error = ParseSyntaxError::RegexCompileError(
        "[[[[[[[[[[[[[[[".to_owned(),
        LoadingError::BadPath.into(),
    );
    assert_display(error.source().unwrap(), "Invalid path");
}

#[test]
fn loading_error_parse_syntax_source() {
    let error = LoadingError::ParseSyntax(
        ParseSyntaxError::RegexCompileError("[a-Z]".to_owned(), LoadingError::BadPath.into()),
        String::from("any-file.sublime-syntax"),
    );
    assert_display(
        error.source().unwrap(),
        "Error while compiling regex '[a-Z]': Invalid path",
    )
}

/// Helper to assert that a given implementation of [Display] generates the
/// expected string.
fn assert_display(display: impl Display, expected_display: &str) {
    assert_eq!(format!("{}", display), String::from(expected_display));
}
