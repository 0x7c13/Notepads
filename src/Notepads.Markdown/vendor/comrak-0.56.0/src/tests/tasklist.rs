use super::*;
use pretty_assertions::assert_eq;

#[test]
fn tasklist() {
    html_opts!(
        [
            render.r#unsafe,
            extension.tasklist,
            parse.relaxed_tasklist_matching
        ],
        concat!(
            "* [ ] Red\n",
            "* [x] Green\n",
            "* [ ] `Blue`\n",
            "* [!] Papayawhip\n",
            "<!-- end list -->\n",
            "1. [ ] Bird\n",
            "2. [ ] McHale\n",
            "3. [x] Parish\n",
            "<!-- end list -->\n",
            "* [ ] Red\n",
            "  * [x] Green\n",
            "    * [ ] Blue\n"
        ),
        concat!(
            "<ul>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> Red</li>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Green</li>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> <code>Blue</code></li>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Papayawhip</li>\n",
            "</ul>\n",
            "<!-- end list -->\n",
            "<ol>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> Bird</li>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> McHale</li>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Parish</li>\n",
            "</ol>\n",
            "<!-- end list -->\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> Red\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Green\n",
            "<ul>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> Blue</li>\n",
            "</ul>\n",
            "</li>\n",
            "</ul>\n",
            "</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_with_classes() {
    html_opts!(
        [
            render.r#unsafe,
            extension.tasklist,
            render.tasklist_classes,
            parse.relaxed_tasklist_matching
        ],
        concat!(
            "* [ ] Red\n",
            "* [x] Green\n",
            "* [ ] Blue\n",
            "* [!] Papayawhip\n",
            "<!-- end list -->\n",
            "1. [ ] Bird\n",
            "2. [ ] McHale\n",
            "3. [x] Parish\n",
            "<!-- end list -->\n",
            "* [ ] Red\n",
            "  * [x] Green\n",
            "    * [ ] Blue\n"
        ),
        concat!(
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Red</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Green</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Blue</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Papayawhip</li>\n",
            "</ul>\n",
            "<!-- end list -->\n",
            "<ol class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Bird</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> McHale</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Parish</li>\n",
            "</ol>\n",
            "<!-- end list -->\n",
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Red\n",
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Green\n",
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> Blue</li>\n",
            "</ul>\n",
            "</li>\n",
            "</ul>\n",
            "</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_relaxed_regression() {
    html_opts!(
        [extension.tasklist, parse.relaxed_tasklist_matching],
        "* [!] Red\n",
        concat!(
            "<ul>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Red</li>\n",
            "</ul>\n"
        ),
    );

    html_opts!(
        [extension.tasklist],
        "* [!] Red\n",
        concat!("<ul>\n", "<li>[!] Red</li>\n", "</ul>\n"),
    );

    html_opts!(
        [extension.tasklist, parse.relaxed_tasklist_matching],
        "* [!] Red\n",
        concat!(
            "<ul>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> Red</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_with_classes_relaxed_regression() {
    html_opts!(
        [
            extension.tasklist,
            render.tasklist_classes,
            parse.relaxed_tasklist_matching
        ],
        "* [!] Red\n",
        concat!(
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Red</li>\n",
            "</ul>\n"
        ),
    );

    html_opts!(
        [extension.tasklist, render.tasklist_classes],
        "* [!] Red\n",
        concat!("<ul>\n", "<li>[!] Red</li>\n", "</ul>\n"),
    );

    html_opts!(
        [
            extension.tasklist,
            render.tasklist_classes,
            parse.relaxed_tasklist_matching
        ],
        "* [!] Red\n",
        concat!(
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> Red</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_32() {
    html_opts!(
        [render.r#unsafe, extension.tasklist],
        concat!(
            "- [ ] List item 1\n",
            "- [ ] This list item is **bold**\n",
            "- [x] There is some `code` here\n"
        ),
        concat!(
            "<ul>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> List item 1</li>\n",
            "<li><input type=\"checkbox\" disabled=\"\" /> This list item is <strong>bold</strong></li>\n",
            "<li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> There is some <code>code</code> here</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_32_with_classes() {
    html_opts!(
        [render.r#unsafe, extension.tasklist, render.tasklist_classes],
        concat!(
            "- [ ] List item 1\n",
            "- [ ] This list item is **bold**\n",
            "- [x] There is some `code` here\n"
        ),
        concat!(
            "<ul class=\"contains-task-list\">\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> List item 1</li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" disabled=\"\" /> This list item is <strong>bold</strong></li>\n",
            "<li class=\"task-list-item\"><input type=\"checkbox\" class=\"task-list-item-checkbox\" checked=\"\" disabled=\"\" /> There is some <code>code</code> here</li>\n",
            "</ul>\n"
        ),
    );
}

#[test]
fn tasklist_in_table() {
    html_opts!(
        [
            extension.tasklist,
            extension.table,
            parse.tasklist_in_table,
            render.sourcepos
        ],
        concat!(
            "|     | name |\n",
            "| --- | ---- |\n",
            "| [ ] | Rell |\n",
            "| [x] | Kai  |\n"
        ),
        concat!(
            "<table data-sourcepos=\"1:1-4:14\">\n",
            "<thead>\n",
            "<tr data-sourcepos=\"1:1-1:14\">\n",
            "<th data-sourcepos=\"1:2-1:6\"></th>\n",
            "<th data-sourcepos=\"1:8-1:13\">name</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr data-sourcepos=\"3:1-3:14\">\n",
            "<td data-sourcepos=\"3:2-3:6\">\n",
            "<input type=\"checkbox\" data-sourcepos=\"3:3-3:5\" disabled=\"\" /> </td>\n",
            "<td data-sourcepos=\"3:8-3:13\">Rell</td>\n",
            "</tr>\n",
            "<tr data-sourcepos=\"4:1-4:14\">\n",
            "<td data-sourcepos=\"4:2-4:6\">\n",
            "<input type=\"checkbox\" data-sourcepos=\"4:3-4:5\" checked=\"\" disabled=\"\" /> </td>\n",
            "<td data-sourcepos=\"4:8-4:13\">Kai</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
        ),
    );

    html_opts!(
        [
            extension.tasklist,
            extension.table,
            parse.tasklist_in_table,
            parse.relaxed_tasklist_matching,
            render.sourcepos
        ],
        concat!(
            "|         | name |\n",
            "| ------- | ---- |\n",
            "| [~]     | Rell |\n",
            "| [x]     | Kai  |\n",
            "| [x] No. | Ez   |\n"
        ),
        concat!(
            "<table data-sourcepos=\"1:1-5:18\">\n",
            "<thead>\n",
            "<tr data-sourcepos=\"1:1-1:18\">\n",
            "<th data-sourcepos=\"1:2-1:10\"></th>\n",
            "<th data-sourcepos=\"1:12-1:17\">name</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr data-sourcepos=\"3:1-3:18\">\n",
            "<td data-sourcepos=\"3:2-3:10\">\n",
            "<input type=\"checkbox\" data-sourcepos=\"3:3-3:5\" checked=\"\" disabled=\"\" /> </td>\n",
            "<td data-sourcepos=\"3:12-3:17\">Rell</td>\n",
            "</tr>\n",
            "<tr data-sourcepos=\"4:1-4:18\">\n",
            "<td data-sourcepos=\"4:2-4:10\">\n",
            "<input type=\"checkbox\" data-sourcepos=\"4:3-4:5\" checked=\"\" disabled=\"\" /> </td>\n",
            "<td data-sourcepos=\"4:12-4:17\">Kai</td>\n",
            "</tr>\n",
            "<tr data-sourcepos=\"5:1-5:18\">\n",
            "<td data-sourcepos=\"5:2-5:10\">[x] No.</td>\n",
            "<td data-sourcepos=\"5:12-5:17\">Ez</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n"
        ),
    );
}

#[test]
fn tasklist_in_table_fuzz() {
    html_opts!(
        [
            extension.tasklist,
            extension.table,
            extension.autolink,
            parse.tasklist_in_table,
            parse.ignore_setext
        ],
        "o\n-\t\r[ ] W@W.I[ ] ",
        concat!(
            "<table>\n",
            "<thead>\n",
            "<tr>\n",
            "<th>o</th>\n",
            "</tr>\n",
            "</thead>\n",
            "<tbody>\n",
            "<tr>\n",
            "<td>[ ] <a href=\"mailto:W@W.I\">W@W.I</a>[ ]</td>\n",
            "</tr>\n",
            "</tbody>\n",
            "</table>\n",
        ),
    );
}

#[test]
fn sourcepos() {
    assert_ast_match!(
        [],
        "h\n"
        "- [ ] xy\n"
        "  - [x] zw\n",
        (document (1:1-3:10) [
            (paragraph (1:1-1:1) [
                (text (1:1-1:1) "h")
            ])
            (list (2:1-3:10) [
                (item (2:1-3:10) [
                    (paragraph (2:3-2:8) [
                        (text (2:3-2:8) "[ ] xy")
                    ])
                    (list (3:3-3:10) [
                        (item (3:3-3:10) [
                            (paragraph (3:5-3:10) [
                                (text (3:5-3:10) "[x] zw")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );

    // https://github.github.com/gfm/#example-279
    assert_ast_match!(
        [extension.tasklist],
        "- [ ] item\n",
        (document (1:1-1:10) [
            (list (1:1-1:10) [
                (taskitem (1:1-1:10) [
                    (paragraph (1:7-1:10) [
                        (text (1:7-1:10) "item")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist],
        "- [ ] item\n"
        "- [x] item2\n",
        (document (1:1-2:11) [
            (list (1:1-2:11) [
                (taskitem (1:1-1:10) [
                    (paragraph (1:7-1:10) [
                        (text (1:7-1:10) "item")
                    ])
                ])
                (taskitem (2:1-2:11) [
                    (paragraph (2:7-2:11) [
                        (text (2:7-2:11) "item2")
                    ])
                ])
            ])
        ])
    );

    // https://github.github.com/gfm/#example-280
    assert_ast_match!(
        [extension.tasklist],
        "- [x] item\n"
        "  - [ ] item2\n"
        "  - [x] item3\n"
        "- [ ] item4\n",
        (document (1:1-4:11) [
            (list (1:1-4:11) [
                (taskitem (1:1-3:13) [
                    (paragraph (1:7-1:10) [
                        (text (1:7-1:10) "item")
                    ])
                    (list (2:3-3:13) [
                        (taskitem (2:3-2:13) [
                            (paragraph (2:9-2:13) [
                                (text (2:9-2:13) "item2")
                            ])
                        ])
                        (taskitem (3:3-3:13) [
                            (paragraph (3:9-3:13) [
                                (text (3:9-3:13) "item3")
                            ])
                        ])
                    ])
                ])
                (taskitem (4:1-4:11) [
                    (paragraph (4:7-4:11) [
                        (text (4:7-4:11) "item4")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist],
        "- [ ] bullet point one\n"
        "- bullet point two and some extra text\n"
        "- [x] bullet point three\n",
        (document (1:1-3:24) [
            (list (1:1-3:24) [
                (taskitem (1:1-1:22) [
                    (paragraph (1:7-1:22) [
                        (text (1:7-1:22) "bullet point one")
                    ])
                ])
                (item (2:1-2:38) [
                    (paragraph (2:3-2:38) [
                        (text (2:3-2:38) "bullet point two and some extra text")
                    ])
                ])
                (taskitem (3:1-3:24) [
                    (paragraph (3:7-3:24) [
                        (text (3:7-3:24) "bullet point three")
                    ])
                ])
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist],
        "- [ ] bullet point one\n"
        "- bullet point two and some extra text\n"
        "- [x] bullet point three\n"
        "\n"
        "hello world\n",
        (document (1:1-5:11) [
            (list (1:1-3:24) [
                (taskitem (1:1-1:22) [
                    (paragraph (1:7-1:22) [
                        (text (1:7-1:22) "bullet point one")
                    ])
                ])
                (item (2:1-2:38) [
                    (paragraph (2:3-2:38) [
                        (text (2:3-2:38) "bullet point two and some extra text")
                    ])
                ])
                (taskitem (3:1-3:24) [
                    (paragraph (3:7-3:24) [
                        (text (3:7-3:24) "bullet point three")
                    ])
                ])
            ])
            (paragraph (5:1-5:11) [
                (text (5:1-5:11) "hello world")
            ])
        ])
    );

    assert_ast_match!(
        [extension.tasklist],
        "h\n"
        "- [ ] xy\n"
        "  - [x] zw\n",
        (document (1:1-3:10) [
            (paragraph (1:1-1:1) [
                (text (1:1-1:1) "h")
            ])
            (list (2:1-3:10) [
                (taskitem (2:1-3:10) [
                    (paragraph (2:7-2:8) [
                        (text (2:7-2:8) "xy")
                    ])
                    (list (3:3-3:10) [
                        (taskitem (3:3-3:10) [
                            (paragraph (3:9-3:10) [
                                (text (3:9-3:10) "zw")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn tasklist_relaxed_unicode() {
    assert_ast_match!(
        [extension.tasklist, parse.relaxed_tasklist_matching],
        "- [あ] xy\n" // U+3042
        "  - [い] zw\n", // U+3044
        (document (1:1-2:12) [
            (list (1:1-2:12) [
                (taskitem (1:1-2:12) [
                    (paragraph (1:9-1:10) [
                        (text (1:9-1:10) "xy")
                    ])
                    (list (2:3-2:12) [
                        (taskitem (2:3-2:12) [
                            (paragraph (2:11-2:12) [
                                (text (2:11-2:12) "zw")
                            ])
                        ])
                    ])
                ])
            ])
        ])
    );
}

#[test]
fn tasklist_symbol_sourcepos() {
    let mut options = Options::default();
    options.extension.tasklist = true;

    let arena = Arena::new();
    let md = r#"Henlo!

> - [x] well
>
>   1. * > 3. [ ] true

+ [x] ok
"#;

    let root = parse_document(&arena, md, &options);
    let sourceposes = root
        .descendants()
        .filter_map(|n| {
            let NodeValue::TaskItem(ref nti) = n.data().value else {
                return None;
            };
            Some(nti.symbol_sourcepos)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sourceposes,
        vec![
            (3, 6, 3, 6).into(),
            (5, 16, 5, 16).into(),
            (7, 4, 7, 4).into()
        ]
    );
}

#[test]
fn tasklist_symbol_sourcepos_relaxed() {
    let mut options = Options::default();
    options.extension.tasklist = true;
    options.extension.table = true;
    options.parse.relaxed_tasklist_matching = true;
    options.parse.tasklist_in_table = true;

    let arena = Arena::new();
    let md = r#"Henlo!

> - [あ] well
>
>   1. * > 3. [和] true

| hm   |    |
| ---- | -- |
| [ワ] | ok |
"#;

    let root = parse_document(&arena, md, &options);
    let sourceposes = root
        .descendants()
        .filter_map(|n| {
            let NodeValue::TaskItem(ref nti) = n.data().value else {
                return None;
            };
            Some(nti.symbol_sourcepos)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sourceposes,
        vec![
            (3, 6, 3, 8).into(),
            (5, 16, 5, 18).into(),
            (9, 4, 9, 6).into()
        ]
    );
}
