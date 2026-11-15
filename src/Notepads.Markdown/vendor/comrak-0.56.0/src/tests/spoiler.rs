use super::*;

#[test]
fn spoiler() {
    html_opts!(
        [extension.spoiler],
        "The ||dog dies at the end of Marley and Me||.\n",
        "<p>The <span class=\"spoiler\">dog dies at the end of Marley and Me</span>.</p>\n",
    );
}

#[test]
fn spoiler_in_table() {
    html_opts!(
        [extension.table, extension.spoiler],
        "Text | Result\n--- | ---\n`||some clever text||` | ||some clever text||\n",
        concat!(
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>Text</th>\n",
            "<th>Result</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td><code>||some clever text||</code></td>\n",
            "<td><span class=\"spoiler\">some clever text</span></td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n"
        ),
    );
}

#[test]
fn spoiler_regressions() {
    html_opts!(
        [extension.spoiler],
        "|should not be spoiler|\n||should be spoiler||\n|||should be spoiler surrounded by pipes|||",
        concat!(
            "<p>|should not be spoiler|\n",
            "<span class=\"spoiler\">should be spoiler</span>\n",
            "|<span class=\"spoiler\">should be spoiler surrounded by pipes</span>|</p>\n"
        ),
    );
}

#[test]
fn mismatched_spoilers() {
    html_opts!(
        [extension.spoiler],
        "|||this is a spoiler with pipe in front||\n||this is not a spoiler|\n||this is a spoiler with pipe after|||",
        concat!(
            "<p>|<span class=\"spoiler\">this is a spoiler with pipe in front</span>\n",
            "||this is not a spoiler|\n",
            "<span class=\"spoiler\">this is a spoiler with pipe after</span>|</p>\n"
        ),
    );
}
