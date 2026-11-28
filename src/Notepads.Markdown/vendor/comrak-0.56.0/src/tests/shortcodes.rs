#![cfg(feature = "shortcodes")]

use super::*;

#[test]
fn emojis() {
    // Test match
    html_opts!(
        [extension.shortcodes],
        "Hello, happy days! :smile:\n",
        "<p>Hello, happy days! 😄</p>\n",
    );

    // Test match
    html_opts!(
        [extension.shortcodes],
        ":smile::smile::smile::smile:\n",
        "<p>😄😄😄😄</p>\n",
    );

    // Test match
    html_opts!(
        [extension.shortcodes],
        ":smile:::smile:::smile:::smile:\n",
        "<p>😄:😄:😄:😄</p>\n",
    );

    // Test no match
    html_opts!(
        [extension.shortcodes],
        "Hello, happy days! :diego:\n",
        "<p>Hello, happy days! :diego:</p>\n",
    );
}

#[test]
fn emojis_with_autolink_sourcepos() {
    html_opts!(
        [extension.shortcodes, extension.autolink],
        "x :dog: y http://a.b/c z :// w\n",
        "<p>x 🐶 y <a href=\"http://a.b/c\">http://a.b/c</a> z :// w</p>\n",
    );
}

#[test]
fn emojis_specials() {
    // Take a quick trip to https://raw.githubusercontent.com/github/gemoji/master/db/emoji.json
    // with `jq -r .[].aliases[] | sort | grep -E '[^a-z_-]'` to see what else there is to see.
    html_opts!(
        [extension.shortcodes],
        ":+1: :-1: :clock12::1234: :1st_place_medal: :e-mail: :non-potable_water:",
        "<p>👍 👎 🕛🔢 🥇 📧 🚱</p>\n",
    );
}
