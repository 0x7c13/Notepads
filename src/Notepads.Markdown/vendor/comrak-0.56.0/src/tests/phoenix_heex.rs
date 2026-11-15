// https://github.com/phoenixframework/tree-sitter-heex/tree/6603380caf806b3e6c7f0bf61627bb47023d79f1/test/corpus

#![cfg(feature = "phoenix_heex")]

use super::*;

// ============================================================================
// text.txt
// ============================================================================

#[test]
fn plain_text() {
    html_opts!(
        [extension.phoenix_heex],
        "Hello World!\n",
        "<p>Hello World!</p>\n",
    );
}

// ============================================================================
// comments.txt
// ============================================================================

#[test]
fn comments() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "I am some text!\n",
            "<%# Look a comment! %>\n",
            "I am more some text!\n",
        ),
        concat!(
            "<p>I am some text!\n",
            "<%# Look a comment! %>\n",
            "I am more some text!</p>\n",
        ),
    );
}

#[test]
fn empty_comments() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("I am some text!\n", "<%#  %>\n", "I am more some text!\n",),
        concat!(
            "<p>I am some text!\n",
            "<%#  %>\n",
            "I am more some text!</p>\n",
        ),
    );
}

#[test]
fn multi_line_comments() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<%# I\n",
            "    AM\n",
            "    A\n",
            "    COMMENT\n",
            "%>\n",
            "and some text!\n",
        ),
        concat!(
            "<%# I\n",
            "    AM\n",
            "    A\n",
            "    COMMENT\n",
            "%>\n",
            "<p>and some text!</p>\n",
        ),
    );
}

#[test]
fn elixir_comments() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<%= foo\n", "    # |> bar()\n", "    |> baz() %>\n",),
        concat!("<%= foo\n", "    # |> bar()\n", "    |> baz() %>\n",),
    );
}

#[test]
fn html_comments() {
    html_opts_i(
        concat!(
            "<p>I am some text!</p>\n",
            "<!-- Look a comment! -->\n",
            "<p>I am more some text!</p>\n",
        ),
        concat!(
            "<p>I am some text!</p>\n",
            "<!-- Look a comment! -->\n",
            "<p>I am more some text!</p>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn empty_html_comments() {
    html_opts_i(
        concat!(
            "<p>I am some text!</p>\n",
            "<!-- -->\n",
            "<p>I am more some text!</p>\n",
        ),
        concat!(
            "<p>I am some text!</p>\n",
            "<!-- -->\n",
            "<p>I am more some text!</p>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn new_style_multi_line_comments() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<%!--\n",
            "  Where'd the directive go?\n",
            "  <%= @x %>\n",
            "--%>\n",
        ),
        concat!(
            "<%!--\n",
            "  Where'd the directive go?\n",
            "  <%= @x %>\n",
            "--%>\n",
        ),
    );
}

#[test]
fn empty_new_style_multi_line_comments() {
    html_opts!([extension.phoenix_heex], "<%!-- --%>\n", "<%!-- --%>\n",);
}

// ============================================================================
// expressions.txt
// ============================================================================

#[test]
fn simple_expression() {
    html_opts_i(
        "<div hello={@hello} {@world}/>\n",
        "<div hello={@hello} {@world}/>\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn single_empty_tuple() {
    html_opts_i(
        "<div hello={{}} />\n",
        "<div hello={{}} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn nested_empty_tuples() {
    html_opts_i(
        "<div hello={{{{{}}}}} />\n",
        "<div hello={{{{{}}}}} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn interpolation() {
    html_opts_i(
        "<div id={ \"##{@id}\" } />\n",
        "<div id={ \"##{@id}\" } />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn many_openings_and_closings() {
    html_opts_i(
        "<div hello={{{1}, {2}}} />\n",
        "<div hello={{{1}, {2}}} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn interpolation_inside_body() {
    html_opts_i(
        concat!(
            "<div>\n",
            "  { @message }\n",
            "  {@message}\n",
            "  {\"#{1}\"}\n",
            "</div>\n",
        ),
        concat!(
            "<div>\n",
            "  { @message }\n",
            "  {@message}\n",
            "  {\"#{1}\"}\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// directives.txt
// ============================================================================

#[test]
fn if_expression_spread_between_multiple_directives() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<%= if true do %>\n", "  <%= @x %>\n", "<% end %>\n",),
        concat!("<%= if true do %>\n", "  <%= @x %>\n", "<% end %>\n",),
    );
}

#[test]
fn case_expression_spread_between_multiple_directives() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<%= case @x do %>\n",
            "  <%= ^x -> %>X, <%= x %>\n",
            "  <%= _ -> %>Not X\n",
            "<% end %>\n",
        ),
        concat!(
            "<%= case @x do %>\n",
            "  <%= ^x -> %>X, <%= x %>\n",
            "  <%= _ -> %>Not X\n",
            "<% end %>\n",
        ),
    );
}

// ============================================================================
// components.txt
// ============================================================================

#[test]
fn component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<Root.render>\n", "</Root.render>\n",),
        concat!("<Root.render>\n", "</Root.render>\n",),
    );
}

#[test]
fn self_closing_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<MyApp.Components.Root.render/>\n",
        "<MyApp.Components.Root.render/>\n",
    );
}

#[test]
fn nested_components() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<Root.render>\n",
            "  <Grid>\n",
            "    <Card />\n",
            "  </Card>\n",
            "</Root.render>\n",
        ),
        concat!(
            "<Root.render>\n",
            "  <Grid>\n",
            "    <Card />\n",
            "  </Card>\n",
            "</Root.render>\n",
        ),
    );
}

#[test]
fn function_with_module_remote_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<MyComponent.btn text=\"Save\" />\n",
        "<MyComponent.btn text=\"Save\" />\n",
    );
}

#[test]
fn function_without_remote_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.btn text=\"Save\" />\n",
        "<.btn text=\"Save\" />\n",
    );
}

#[test]
fn simple_example() {
    html_opts_i(
        concat!(
            "<div class={@class} title=\"My div\">\n",
            "  <SomeModule.some_func_component attr1=\"some string\" attr2={@some_expression} {@other_dynamic_attrs} />\n",
            "  <.some_local_func_component attr1=\"some string\" />\n",
            "</div>\n",
        ),
        concat!(
            "<div class={@class} title=\"My div\">\n",
            "  <SomeModule.some_func_component attr1=\"some string\" attr2={@some_expression} {@other_dynamic_attrs} />\n",
            "  <.some_local_func_component attr1=\"some string\" />\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// slots.txt
// ============================================================================

#[test]
fn named_slots() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.modal>\n",
            "  <:header>\n",
            "    This is the top of the modal.\n",
            "  </:header>\n",
            "  This is the content of the modal.\n",
            "</.modal>\n",
        ),
        concat!(
            "<.modal>\n",
            "  <:header>\n",
            "    This is the top of the modal.\n",
            "  </:header>\n",
            "  This is the content of the modal.\n",
            "</.modal>\n",
        ),
    );
}

#[test]
fn named_slots_with_attributes() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.modal>\n",
            "  <:header key={@value}>\n",
            "  </:header>\n",
            "</.modal>\n",
        ),
        concat!(
            "<.modal>\n",
            "  <:header key={@value}>\n",
            "  </:header>\n",
            "</.modal>\n",
        ),
    );
}

#[test]
fn self_closing_slot_does_not_error() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.modal>\n", "  <:header />\n", "</.modal>\n",),
        concat!("<.modal>\n", "  <:header />\n", "</.modal>\n",),
    );
}

// ============================================================================
// special_attributes.txt
// ============================================================================

#[test]
fn component_special_attribute_let() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.form :let={f}>\n",
            "    <%= text_input f, :text %>\n",
            "    <%= submit \"Submit\" %>\n",
            "</.form>\n",
        ),
        concat!(
            "<.form :let={f}>\n",
            "    <%= text_input f, :text %>\n",
            "    <%= submit \"Submit\" %>\n",
            "</.form>\n",
        ),
    );
}

#[test]
fn slot_special_attribute_let() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.func>\n",
            "  <:slot :let={foo}>\n",
            "    <%= foo %>\n",
            "  </:slot>\n",
            "</.func>\n",
        ),
        concat!(
            "<.func>\n",
            "  <:slot :let={foo}>\n",
            "    <%= foo %>\n",
            "  </:slot>\n",
            "</.func>\n",
        ),
    );
}

#[test]
fn tag_special_attribute_for() {
    html_opts_i(
        concat!(
            "<div :for={item <- @items}>\n",
            "  <%= item %>\n",
            "</div>\n",
        ),
        concat!(
            "<div :for={item <- @items}>\n",
            "  <%= item %>\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn self_closing_tag_special_attribute_for() {
    html_opts_i(
        "<div :for={item <- @items} />\n",
        "<div :for={item <- @items} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn self_closing_tag_special_attribute_for_no_space() {
    html_opts_i(
        "<div :for={item <- @items}/>\n",
        "<div :for={item <- @items}/>\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn component_special_attribute_for() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component :for={item <- @items}>\n",
            "  <%= item %>\n",
            "</.component>\n",
        ),
        concat!(
            "<.component :for={item <- @items}>\n",
            "  <%= item %>\n",
            "</.component>\n",
        ),
    );
}

#[test]
fn self_closing_component_special_attribute_for() {
    html_opts!(
        [extension.phoenix_heex],
        "<.component :for={item <- @items}/>\n",
        "<.component :for={item <- @items}/>\n",
    );
}

#[test]
fn slot_special_attribute_for() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component>\n",
            "  <:slot :for={item <- @items}>\n",
            "    <%= item %>\n",
            "  </:slot>\n",
            "</.component>\n",
        ),
        concat!(
            "<.component>\n",
            "  <:slot :for={item <- @items}>\n",
            "    <%= item %>\n",
            "  </:slot>\n",
            "</.component>\n",
        ),
    );
}

#[test]
fn self_closing_slot_special_attribute_for() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component>\n",
            "  <:slot :for={item <- @items} />\n",
            "</.component>\n",
        ),
        concat!(
            "<.component>\n",
            "  <:slot :for={item <- @items} />\n",
            "</.component>\n",
        ),
    );
}

#[test]
fn tag_special_attribute_stream() {
    html_opts_i(
        concat!(
            "<div :stream={item <- @items}>\n",
            "  <%= item %>\n",
            "</div>\n",
        ),
        concat!(
            "<div :stream={item <- @items}>\n",
            "  <%= item %>\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn tag_special_attribute_if() {
    html_opts_i(
        concat!("<div :if={@item}>\n", "  <%= @item %>\n", "</div>\n",),
        concat!("<div :if={@item}>\n", "  <%= @item %>\n", "</div>\n",),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn self_closing_tag_special_attribute_if() {
    html_opts_i(
        "<div :if={@item} />\n",
        "<div :if={@item} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn component_special_attribute_if() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component :if={@expression}>\n",
            "  <%= @expression %>\n",
            "</.component>\n",
        ),
        concat!(
            "<.component :if={@expression}>\n",
            "  <%= @expression %>\n",
            "</.component>\n",
        ),
    );
}

#[test]
fn self_closing_component_special_attribute_if() {
    html_opts!(
        [extension.phoenix_heex],
        "<.component :if={@expression} />\n",
        "<.component :if={@expression} />\n",
    );
}

#[test]
fn slot_special_attribute_if() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component>\n",
            "  <:slot :if={@expression}>\n",
            "    <%= @expression %>\n",
            "  </:slot>\n",
            "</.component>\n",
        ),
        concat!(
            "<.component>\n",
            "  <:slot :if={@expression}>\n",
            "    <%= @expression %>\n",
            "  </:slot>\n",
            "</.component>\n",
        ),
    );
}

#[test]
fn self_closing_slot_special_attribute_if() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component>\n",
            "  <:slot :if={@expression} />\n",
            "</.component>\n",
        ),
        concat!(
            "<.component>\n",
            "  <:slot :if={@expression} />\n",
            "</.component>\n",
        ),
    );
}

// ============================================================================
// tags.txt
// ============================================================================

#[test]
fn html_tag() {
    html_opts_i(
        concat!("<div>\n", "</div>\n",),
        concat!("<div>\n", "</div>\n",),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn html_self_closing_tag() {
    html_opts_i("<div/>\n", "<div/>\n", true, |opts| {
        opts.extension.phoenix_heex = true;
        opts.render.r#unsafe = true;
    });
}

#[test]
fn html_nested_tags() {
    html_opts_i(
        concat!(
            "<div>\n",
            "  <div>\n",
            "    <div />\n",
            "  </div>\n",
            "</div>\n",
        ),
        concat!(
            "<div>\n",
            "  <div>\n",
            "    <div />\n",
            "  </div>\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn doctype() {
    html_opts_i(
        concat!(
            "<!DOCTYPE html>\n",
            "\n",
            "<html lang=\"en\">\n",
            "  <body>\n",
            "  <%= @inner_content %>\n",
            "  </body> \n",
            "</html>\n",
        ),
        concat!(
            "<!DOCTYPE html>\n",
            "<html lang=\"en\">\n",
            "  <body>\n",
            "  <%= @inner_content %>\n",
            "  </body> \n",
            "</html>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn tag_name_with_hyphen() {
    html_opts_i(
        concat!("<box-icon name='like'>\n", "</box-icon>\n",),
        concat!("<box-icon name='like'>\n", "</box-icon>\n",),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// attributes.txt
// ============================================================================

#[test]
fn unquoted_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render key=value />\n",
        "<Root.render key=value />\n",
    );
}

#[test]
fn boolean_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render hidden />\n",
        "<Root.render hidden />\n",
    );
}

#[test]
fn single_quoted_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render key='value' />\n",
        "<Root.render key='value' />\n",
    );
}

#[test]
fn double_quoted_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render key=\"value\" />\n",
        "<Root.render key=\"value\" />\n",
    );
}

#[test]
fn expression_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render key={value} />\n",
        "<Root.render key={value} />\n",
    );
}

#[test]
fn single_character_attribute() {
    html_opts!(
        [extension.phoenix_heex],
        "<Root.render a=\"a\" />\n",
        "<Root.render a=\"a\" />\n",
    );
}

#[test]
fn alpine_directive() {
    html_opts_i(
        concat!(
            "<div x-data=\"{ open: false }\">\n",
            "    <button @click=\"open = ! open\">Toggle</button>\n",
            "    <div x-show=\"open\" @click.outside=\"open = false\">Contents...</div>\n",
            "</div>\n",
        ),
        concat!(
            "<div x-data=\"{ open: false }\">\n",
            "    <button @click=\"open = ! open\">Toggle</button>\n",
            "    <div x-show=\"open\" @click.outside=\"open = false\">Contents...</div>\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn alpine_attribute() {
    html_opts_i(
        concat!(
            "<button x-on:click=\"open = ! open\">\n",
            "  Toggle\n",
            "</button>\n",
        ),
        concat!(
            "<button x-on:click=\"open = ! open\">\n",
            "  Toggle\n",
            "</button>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// markdown mixed
// ============================================================================

#[test]
fn markdown_mixed_heading_and_component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("# Contact Form\n", "\n", "<.form>\n", "</.form>\n",),
        concat!("<h1>Contact Form</h1>\n", "<.form>\n", "</.form>\n",),
    );
}

#[test]
fn markdown_mixed_complex_form() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "# Contact Form\n",
            "\n",
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "  <.input field={@form[:email]} />\n",
            "</.form>\n",
            "\n",
            "## Footer\n",
            "\n",
            "Socials\n",
        ),
        concat!(
            "<h1>Contact Form</h1>\n",
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "  <.input field={@form[:email]} />\n",
            "</.form>\n",
            "<h2>Footer</h2>\n",
            "<p>Socials</p>\n",
        ),
    );
}

#[test]
fn markdown_mixed_inline_expression_and_component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "# Posts from {Date.utc_today().year}\n",
            "\n",
            "<.button>Submit</.button>\n",
        ),
        concat!(
            "<h1>Posts from {Date.utc_today().year}</h1>\n",
            "<.button>Submit</.button>\n",
        ),
    );
}

// ============================================================================
// inline components
// ============================================================================

#[test]
fn inline_function_component() {
    html_opts!(
        [extension.phoenix_heex],
        "Click <.button>here</.button> to continue\n",
        "<p>Click <.button>here</.button> to continue</p>\n",
    );
}

#[test]
fn markdown_after_inline_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.component>first</.component> **second**\n",
        "<p><.component>first</.component> <strong>second</strong></p>\n",
    );
}

#[test]
fn inline_nested_components() {
    html_opts!(
        [extension.phoenix_heex],
        "<.outer><.inner>text</.inner></.outer>\n",
        "<.outer><.inner>text</.inner></.outer>\n",
    );
}

#[test]
fn markdown_after_inline_nested_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.outer><.inner>text</.inner></.outer> **after**\n",
        "<p><.outer><.inner>text</.inner></.outer> <strong>after</strong></p>\n",
    );
}

#[test]
fn markdown_after_multiple_inline_components() {
    html_opts!(
        [extension.phoenix_heex],
        "<.a>x</.a><.b>y</.b> **after**\n",
        "<p><.a>x</.a><.b>y</.b> <strong>after</strong></p>\n",
    );
}

#[test]
fn markdown_after_self_closing_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.icon name=\"star\" /> **label**\n",
        "<p><.icon name=\"star\" /> <strong>label</strong></p>\n",
    );
}

#[test]
fn markdown_after_component_with_attributes() {
    html_opts!(
        [extension.phoenix_heex],
        "<.link href=\"/\">click</.link> **bold**\n",
        "<p><.link href=\"/\">click</.link> <strong>bold</strong></p>\n",
    );
}

#[test]
fn markdown_after_empty_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.spacer></.spacer> **after**\n",
        "<p><.spacer></.spacer> <strong>after</strong></p>\n",
    );
}

#[test]
fn code_blocks_mixed_with_components() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "# Hello World\n",
            "\n",
            "<.link href=\"/\">Regular anchor link</.link>\n",
            "\n",
            "```elixir\n",
            "IO.puts(\"Hello\")\n",
            "```\n",
            "\n",
            "<.link navigate=\"/?sort=asc\" replace={false}>\n",
            "  Sort By Price\n",
            "</.link>\n",
            "\n",
            "```rust\n",
            "let result = ammonia::clean(\"<b><img src='' onerror=alert('attack')>I'm not trying to XSS you</b>\");\n",
            "```\n",
        ),
        concat!(
            "<h1>Hello World</h1>\n",
            "<.link href=\"/\">Regular anchor link</.link>\n",
            "<pre><code class=\"language-elixir\">IO.puts(&quot;Hello&quot;)\n",
            "</code></pre>\n",
            "<.link navigate=\"/?sort=asc\" replace={false}>\n",
            "  Sort By Price\n",
            "</.link>\n",
            "<pre><code class=\"language-rust\">let result = ammonia::clean(&quot;&lt;b&gt;&lt;img src='' onerror=alert('attack')&gt;I'm not trying to XSS you&lt;/b&gt;&quot;);\n",
            "</code></pre>\n",
        ),
    );
}

#[test]
fn inline_slot() {
    // Slots are invalid inline (they only appear inside components)
    // so they should be HTML-escaped
    html_opts!(
        [extension.phoenix_heex],
        "Header: <:title>My Title</:title>\n",
        "<p>Header: &lt;:title&gt;My Title&lt;/:title&gt;</p>\n",
    );
}

// ============================================================================
// inline expressions
// ============================================================================

#[test]
fn inline_expression_simple() {
    html_opts!(
        [extension.phoenix_heex],
        "The year is {Date.utc_today().year}\n",
        "<p>The year is {Date.utc_today().year}</p>\n",
    );
}

#[test]
fn inline_expression_in_heading() {
    html_opts!(
        [extension.phoenix_heex],
        "# Posts from {Date.utc_today().year}\n",
        "<h1>Posts from {Date.utc_today().year}</h1>\n",
    );
}

#[test]
fn inline_expression_nested_braces() {
    html_opts!(
        [extension.phoenix_heex],
        "Value: {%{key: \"value\"}}\n",
        "<p>Value: {%{key: \"value\"}}</p>\n",
    );
}

#[test]
fn inline_expression_with_function_call() {
    html_opts!(
        [extension.phoenix_heex],
        "Count: {length(items)}\n",
        "<p>Count: {length(items)}</p>\n",
    );
}

#[test]
fn inline_expression_multiple() {
    html_opts!(
        [extension.phoenix_heex],
        "User {user.name} has {user.age} items\n",
        "<p>User {user.name} has {user.age} items</p>\n",
    );
}

#[test]
fn inline_expression_with_string_literal() {
    html_opts!(
        [extension.phoenix_heex],
        "Text: {\"hello {world}\"}\n",
        "<p>Text: {\"hello {world}\"}</p>\n",
    );
}

// ============================================================================
// inline directives
// ============================================================================

#[test]
fn directive_simple() {
    html_opts!(
        [extension.phoenix_heex],
        "The year is <% Date.utc_today().year %>\n",
        "<p>The year is <% Date.utc_today().year %></p>\n",
    );
}

#[test]
fn directive_output() {
    html_opts!(
        [extension.phoenix_heex],
        "The year is <%= Date.utc_today().year %>\n",
        "<p>The year is <%= Date.utc_today().year %></p>\n",
    );
}

#[test]
fn directive_escaped() {
    html_opts!(
        [extension.phoenix_heex],
        "Code: <%% IO.inspect(user) %>\n",
        "<p>Code: <%% IO.inspect(user) %></p>\n",
    );
}

#[test]
fn directive_escaped_output() {
    html_opts!(
        [extension.phoenix_heex],
        "Code: <%%= IO.inspect(user) %>\n",
        "<p>Code: <%%= IO.inspect(user) %></p>\n",
    );
}

#[test]
fn directive_with_string() {
    html_opts!(
        [extension.phoenix_heex],
        "Text: <%= \"hello %> world\" %>\n",
        "<p>Text: <%= \"hello %> world\" %></p>\n",
    );
}

#[test]
fn directive_in_heading() {
    html_opts!(
        [extension.phoenix_heex],
        "# Posts from <%= @year %>\n",
        "<h1>Posts from <%= @year %></h1>\n",
    );
}

#[test]
fn directive_multiple() {
    html_opts!(
        [extension.phoenix_heex],
        "User <%= @user.name %> has <%= @user.age %> years\n",
        "<p>User <%= @user.name %> has <%= @user.age %> years</p>\n",
    );
}

#[test]
fn directive_multiline() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "Result: <%=\n",
            "  user.name\n",
            "  |> String.upcase()\n",
            "%>\n",
        ),
        concat!(
            "<p>Result: <%=\n",
            "  user.name\n",
            "  |> String.upcase()\n",
            "%></p>\n",
        ),
    );
}

#[test]
fn directive_mixed_with_expressions() {
    html_opts!(
        [extension.phoenix_heex],
        "Value: {user.name} or <%= @user.name %>\n",
        "<p>Value: {user.name} or <%= @user.name %></p>\n",
    );
}

// ============================================================================
// special attributes combinations
// ============================================================================

#[test]
fn special_attributes_for_with_html_tag() {
    html_opts_i(
        concat!(
            "<div :for={item <- @items}>\n",
            "  {item.name}\n",
            "</div>\n",
        ),
        concat!(
            "<div :for={item <- @items}>\n",
            "  {item.name}\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn special_attributes_if_with_html_tag() {
    html_opts_i(
        concat!(
            "<div :if={@show_message}>\n",
            "  Message here\n",
            "</div>\n",
        ),
        concat!(
            "<div :if={@show_message}>\n",
            "  Message here\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn special_attributes_let_with_component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.form :let={f} for={@changeset}>\n",
            "  <.input field={f[:email]} />\n",
            "</.form>\n",
        ),
        concat!(
            "<.form :let={f} for={@changeset}>\n",
            "  <.input field={f[:email]} />\n",
            "</.form>\n",
        ),
    );
}

#[test]
fn special_attributes_stream_with_html_tag() {
    html_opts_i(
        concat!(
            "<div :stream={@messages}>\n",
            "  {message.text}\n",
            "</div>\n",
        ),
        concat!(
            "<div :stream={@messages}>\n",
            "  {message.text}\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn special_attributes_slot_with_let() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.table rows={@users}>\n",
            "  <:col :let={user} label=\"Name\">\n",
            "    {user.name}\n",
            "  </:col>\n",
            "  <:col :let={user} label=\"Email\">\n",
            "    {user.email}\n",
            "  </:col>\n",
            "</.table>\n",
        ),
        concat!(
            "<.table rows={@users}>\n",
            "  <:col :let={user} label=\"Name\">\n",
            "    {user.name}\n",
            "  </:col>\n",
            "  <:col :let={user} label=\"Email\">\n",
            "    {user.email}\n",
            "  </:col>\n",
            "</.table>\n",
        ),
    );
}

#[test]
fn special_attributes_for_and_if_combined() {
    html_opts_i(
        concat!(
            "<ul>\n",
            "  <li :for={user <- @users} :if={user.active}>\n",
            "    {user.name}\n",
            "  </li>\n",
            "</ul>\n",
        ),
        concat!(
            "<ul>\n",
            "  <li :for={user <- @users} :if={user.active}>\n",
            "    {user.name}\n",
            "  </li>\n",
            "</ul>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn special_attributes_nested() {
    html_opts_i(
        concat!(
            "<div :if={@show_list}>\n",
            "  <div :for={item <- @items}>\n",
            "    {item.name}\n",
            "  </div>\n",
            "</div>\n",
        ),
        concat!(
            "<div :if={@show_list}>\n",
            "  <div :for={item <- @items}>\n",
            "    {item.name}\n",
            "  </div>\n",
            "</div>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// root attributes
// ============================================================================

#[test]
fn root_attributes_spread() {
    html_opts_i(
        concat!("<div class=\"base\" {@attrs}>\n", "  Content\n", "</div>\n",),
        concat!("<div class=\"base\" {@attrs}>\n", "  Content\n", "</div>\n",),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn root_attributes_multiple() {
    html_opts_i(
        "<.component class={@class} {@attrs1} hidden {@attrs2} />\n",
        "<.component class={@class} {@attrs1} hidden {@attrs2} />\n",
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// escaping
// ============================================================================

#[test]
fn escaped_braces_in_string() {
    html_opts!(
        [extension.phoenix_heex],
        "Value: {\"\\{escaped}\"}\n",
        "<p>Value: {\"\\{escaped}\"}</p>\n",
    );
}

// ============================================================================
// attributes
// ============================================================================

#[test]
fn boolean_attributes_phoenix_component() {
    html_opts!(
        [extension.phoenix_heex],
        "<.input field={@form[:email]} required />\n",
        "<.input field={@form[:email]} required />\n",
    );
}

#[test]
fn attributes_with_expressions_and_spaces() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component\n",
            "  class = {get_class()}\n",
            "  id= {@id}\n",
            "  data ={@data}\n",
            "/>\n",
        ),
        concat!(
            "<.component\n",
            "  class = {get_class()}\n",
            "  id= {@id}\n",
            "  data ={@data}\n",
            "/>\n",
        ),
    );
}

#[test]
fn complex_expressions_in_attributes() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.component\n",
            "  value={user.name}\n",
            "  count={length(items)}\n",
            "  data={%{key: \"value\"}}\n",
            "/>\n",
        ),
        concat!(
            "<.component\n",
            "  value={user.name}\n",
            "  count={length(items)}\n",
            "  data={%{key: \"value\"}}\n",
            "/>\n",
        ),
    );
}

// ============================================================================
// components
// ============================================================================

#[test]
fn function_component_with_attributes() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "</.form>\n",
        ),
        concat!(
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "</.form>\n",
        ),
    );
}

#[test]
fn function_component_nested() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "  <.input field={@form[:email]} />\n",
            "</.form>\n",
        ),
        concat!(
            "<.form\n",
            "  for={@form}\n",
            "  phx-change=\"change_name\"\n",
            ">\n",
            "  <.input field={@form[:email]} />\n",
            "</.form>\n",
        ),
    );
}

// ============================================================================
// slots
// ============================================================================

#[test]
fn slot_tags() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<Component>\n",
            "  <:subtitle>\n",
            "    Subtitle content\n",
            "  </:subtitle>\n",
            "  <:actions>\n",
            "    Action buttons\n",
            "  </:actions>\n",
            "</Component>\n",
        ),
        concat!(
            "<Component>\n",
            "  <:subtitle>\n",
            "    Subtitle content\n",
            "  </:subtitle>\n",
            "  <:actions>\n",
            "    Action buttons\n",
            "  </:actions>\n",
            "</Component>\n",
        ),
    );
}

// ============================================================================
// sourcepos
// ============================================================================

#[test]
fn sourcepos_with_content() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "<.form>\n"
        "  <.input />\n"
        "</.form>\n",
        (document (1:1-3:8) [
            (heex_block (1:1-3:8) "<.form>\n  <.input />\n</.form>\n")
        ]),
    );
}

#[test]
fn block_with_content_inside_blockquote_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "> <.form>\n"
        ">   <.input />\n"
        "> </.form>\n",
        (document (1:1-3:10) [
            (block_quote (1:1-3:10) [
                (heex_block (1:3-3:10) "<.form>\n  <.input />\n</.form>\n")
            ])
        ]),
    );
}

#[test]
fn block_with_code_fence() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "<.header>\n"
        "  ```elixir\n"
        "</.header>\n",
        (document (1:1-3:10) [
            (heex_block (1:1-3:10) "<.header>\n  ```elixir\n</.header>\n")
        ]),
    );
}

#[test]
fn block_with_code_fence_inside_blockquote_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "> <.header>\n"
        ">   ```elixir\n"
        "> </.header>\n",
        (document (1:1-3:12) [
            (block_quote (1:1-3:12) [
                (heex_block (1:3-3:12) "<.header>\n  ```elixir\n</.header>\n")
            ])
        ]),
    );
}

#[test]
fn block_with_text_and_paragraph() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "<.header>\n"
        "hello\n"
        "</.header>\n"
        "\n"
        "hello world\n",
        (document (1:1-5:11) [
            (heex_block (1:1-3:10) "<.header>\nhello\n</.header>\n")
            (paragraph (5:1-5:11) [
                (text (5:1-5:11) "hello world")
            ])
        ]),
    );
}

#[test]
fn block_with_text_and_paragraph_inside_blockquote_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "> <.header>\n"
        "> hello\n"
        "> </.header>\n"
        ">\n"
        "> hello world\n",
        (document (1:1-5:13) [
            (block_quote (1:1-5:13) [
                (heex_block (1:3-3:12) "<.header>\nhello\n</.header>\n")
                (paragraph (5:3-5:13) [
                    (text (5:3-5:13) "hello world")
                ])
            ])
        ]),
    );
}

#[test]
fn inline_expression_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "Value: {user.name}\n",
        (document (1:1-1:18) [
            (paragraph (1:1-1:18) [
                (text (1:1-1:7) "Value: ")
                (heex_inline (1:8-1:18) "{user.name}")
            ])
        ]),
    );
}

#[test]
fn inline_expression_inside_blockquote_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "> Here is a value: {user.name}\n",
        (document (1:1-1:30) [
            (block_quote (1:1-1:30) [
                (paragraph (1:3-1:30) [
                    (text (1:3-1:19) "Here is a value: ")
                    (heex_inline (1:20-1:30) "{user.name}")
                ])
            ])
        ]),
    );
}

#[test]
fn directive_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "Value: <%= @user %>\n",
        (document (1:1-1:19) [
            (paragraph (1:1-1:19) [
                (text (1:1-1:7) "Value: ")
                (heex_inline (1:8-1:19) "<%= @user %>")
            ])
        ]),
    );
}

#[test]
fn directive_inside_blockquote_sourcepos() {
    assert_ast_match!(
        [extension.phoenix_heex],
        "> Here is a value: <%= @user %>\n",
        (document (1:1-1:31) [
            (block_quote (1:1-1:31) [
                (paragraph (1:3-1:31) [
                    (text (1:3-1:19) "Here is a value: ")
                    (heex_inline (1:20-1:31) "<%= @user %>")
                ])
            ])
        ]),
    );
}

// ============================================================================
// tag-aware blocks with blank lines
// ============================================================================

#[test]
fn block_with_blank_lines_inside() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.foo>\n", "\n", "\n", "\n", "</.foo>\n",),
        concat!("<.foo>\n", "\n", "\n", "\n", "</.foo>\n",),
    );
}

#[test]
fn module_component_with_blank_lines() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<MyComponent>\n",
            "\n",
            "  Content here\n",
            "\n",
            "</MyComponent>\n",
        ),
        concat!(
            "<MyComponent>\n",
            "\n",
            "  Content here\n",
            "\n",
            "</MyComponent>\n",
        ),
    );
}

#[test]
fn nested_components_with_blank_lines() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.outer>\n",
            "\n",
            "  <.inner>\n",
            "\n",
            "  </.inner>\n",
            "\n",
            "</.outer>\n",
        ),
        concat!(
            "<.outer>\n",
            "\n",
            "  <.inner>\n",
            "\n",
            "  </.inner>\n",
            "\n",
            "</.outer>\n",
        ),
    );
}

#[test]
fn block_ends_on_closing_tag() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "Content\n", "</.form>\n", "\n", "After\n",),
        concat!("<.form>\n", "Content\n", "</.form>\n", "<p>After</p>\n",),
    );
}

#[test]
fn block_with_multiple_consecutive_empty_lines() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<Foo>\n", "\n", "\n", "</Foo>\n",),
        concat!("<Foo>\n", "\n", "\n", "</Foo>\n",),
    );
}

#[test]
fn self_closing_followed_by_content() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.btn />\n", "\n", "After\n"),
        concat!("<.btn />\n", "<p>After</p>\n"),
    );
}

#[test]
fn self_closing_component_followed_by_content() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<Component.render />\n", "\n", "After\n"),
        concat!("<Component.render />\n", "<p>After</p>\n"),
    );
}

// ============================================================================
// output formats
// ============================================================================

#[test]
fn xml_output() {
    xml_opts(
        concat!("<.form>\n", "</.form>\n",),
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE document SYSTEM \"CommonMark.dtd\">\n",
            "<document xmlns=\"http://commonmark.org/xml/1.0\">\n",
            "  <heex_block xml:space=\"preserve\">&lt;.form&gt;\n",
            "&lt;/.form&gt;\n",
            "</heex_block>\n",
            "</document>\n",
        ),
        |opts| {
            opts.extension.phoenix_heex = true;
        },
    );
}

#[test]
fn commonmark_output() {
    let input = concat!("<.form>\n", "</.form>\n",);

    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.phoenix_heex = true;

    let root = parse_document(&arena, input, &options);
    let mut output = String::new();
    cm::format_document(root, &options, &mut output).unwrap();

    compare_strs(
        &output,
        concat!("<.form>\n", "</.form>\n",),
        "commonmark",
        input,
    );
}

// ============================================================================
// edge cases
// ============================================================================

#[test]
fn malformed_closing_tag_incomplete() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "content\n", "</\n", "</.form>\n",),
        concat!("<.form>\n", "content\n", "</\n", "</.form>\n",),
    );
}

#[test]
fn malformed_closing_tag_no_name() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "</>  \n", "</.form>\n",),
        concat!("<.form>\n", "</>  \n", "</.form>\n",),
    );
}

#[test]
fn empty_component_tag_name() {
    html_opts!([extension.phoenix_heex], "<. />\n", "<p>&lt;. /&gt;</p>\n",);
}

#[test]
fn very_long_tag_name() {
    let long_name = "Component".repeat(50);
    let input = format!("<{}>\n</{}>\n", long_name, long_name);
    let expected = format!("<{}>\n</{}>\n", long_name, long_name);

    html_opts!([extension.phoenix_heex], &input, &expected,);
}

// ============================================================================
// edge cases - EOF handling
// ============================================================================

#[test]
fn unclosed_component_at_eof() {
    html_opts!(
        [extension.phoenix_heex],
        "<.form>\n  content\n",
        "<.form>\n  content\n",
    );
}

#[test]
fn unclosed_directive_at_eof() {
    html_opts!(
        [extension.phoenix_heex],
        "<%= foo\n  bar\n",
        "<%= foo\n  bar\n",
    );
}

#[test]
fn unclosed_comment_at_eof() {
    html_opts!(
        [extension.phoenix_heex],
        "<%# this is a comment\n  that never closes\n",
        "<%# this is a comment\n  that never closes\n",
    );
}

// ============================================================================
// edge cases - tag matching
// ============================================================================

#[test]
fn mismatched_component_tags() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "  content\n", "</.button>\n",),
        concat!("<.form>\n", "  content\n", "</.button>\n",),
    );
}

#[test]
fn mismatched_module_component_tags() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<MyApp.Form>\n", "  content\n", "</MyApp.Button>\n",),
        concat!("<MyApp.Form>\n", "  content\n", "</MyApp.Button>\n",),
    );
}

#[test]
fn case_sensitive_matching() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "  content\n", "</.Form>\n",),
        concat!("<.form>\n", "  content\n", "</.Form>\n",),
    );
}

#[test]
fn closing_tag_with_whitespace() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "  content\n", "</  .form>\n",),
        concat!("<.form>\n", "  content\n", "</  .form>\n",),
    );
}

// ============================================================================
// edge cases - nesting
// ============================================================================

#[test]
fn nested_same_name_module_component() {
    // https://github.com/leandrocp/mdex/issues/298
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<MyApp.List.list>\n",
            "  <MyApp.List.li>\n",
            "    content\n",
            "    <MyApp.List.list>\n",
            "      <MyApp.List.li>nested</MyApp.List.li>\n",
            "    </MyApp.List.list>\n",
            "  </MyApp.List.li>\n",
            "</MyApp.List.list>\n",
        ),
        concat!(
            "<MyApp.List.list>\n",
            "  <MyApp.List.li>\n",
            "    content\n",
            "    <MyApp.List.list>\n",
            "      <MyApp.List.li>nested</MyApp.List.li>\n",
            "    </MyApp.List.list>\n",
            "  </MyApp.List.li>\n",
            "</MyApp.List.list>\n",
        ),
    );
}

#[test]
fn nested_same_name_function_component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.list>\n",
            "  <.li>\n",
            "    content\n",
            "    <.list>\n",
            "      <.li>nested</.li>\n",
            "    </.list>\n",
            "  </.li>\n",
            "</.list>\n",
        ),
        concat!(
            "<.list>\n",
            "  <.li>\n",
            "    content\n",
            "    <.list>\n",
            "      <.li>nested</.li>\n",
            "    </.list>\n",
            "  </.li>\n",
            "</.list>\n",
        ),
    );
}

#[test]
fn self_closing_same_name_inside_block() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<MyApp.List.list>\n",
            "  <MyApp.List.list />\n",
            "</MyApp.List.list>\n",
        ),
        concat!(
            "<MyApp.List.list>\n",
            "  <MyApp.List.list />\n",
            "</MyApp.List.list>\n",
        ),
    );
}

#[test]
fn inline_open_close_same_name_inside_block() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.list>\n", "  <.list>inner</.list>\n", "</.list>\n"),
        concat!("<.list>\n", "  <.list>inner</.list>\n", "</.list>\n")
    );
}

#[test]
fn nested_directives_in_component() {
    html_opts!(
        [extension.phoenix_heex],
        concat!(
            "<.form>\n",
            "  <%= if true do %>\n",
            "    <%= @value %>\n",
            "  <% end %>\n",
            "</.form>\n",
        ),
        concat!(
            "<.form>\n",
            "  <%= if true do %>\n",
            "    <%= @value %>\n",
            "  <% end %>\n",
            "</.form>\n",
        ),
    );
}

// ============================================================================
// edge cases - directive finalization
// ============================================================================

#[test]
fn multiple_directives_on_same_line() {
    html_opts!(
        [extension.phoenix_heex],
        "<%= foo %> text <%= bar %>\n",
        "<%= foo %> text <%= bar %>\n",
    );
}

#[test]
fn directive_with_text_after() {
    html_opts!(
        [extension.phoenix_heex],
        "<%= foo %>text after\n",
        "<%= foo %>text after\n",
    );
}

#[test]
fn back_to_back_directives() {
    html_opts!(
        [extension.phoenix_heex],
        "<% foo %><% bar %><% baz %>\n",
        "<% foo %><% bar %><% baz %>\n",
    );
}

#[test]
fn directive_incomplete_on_line() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("text <%= foo\n", "  bar %> after\n",),
        concat!("<p>text <%= foo\n", "  bar %> after</p>\n",),
    );
}

// ============================================================================
// edge cases - empty and whitespace
// ============================================================================

#[test]
fn empty_component_block() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "</.form>\n",),
        concat!("<.form>\n", "</.form>\n",),
    );
}

#[test]
fn component_with_only_whitespace() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.form>\n", "   \n", "  \n", "</.form>\n",),
        concat!("<.form>\n", "   \n", "  \n", "</.form>\n",),
    );
}

#[test]
fn empty_directive() {
    html_opts!([extension.phoenix_heex], "<%%>\n", "<%%>\n",);
}

// ============================================================================
// edge cases - whitespace preservation in different contexts
// ============================================================================

#[test]
fn component_in_list() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("- <.button>\n", "    text\n", "  </.button>\n",),
        concat!(
            "<ul>\n",
            "<li>\n",
            "<.button>\n",
            "  text\n",
            "</.button>\n",
            "</li>\n",
            "</ul>\n",
        ),
    );
}

// ============================================================================
// edge cases - interaction with other extensions
// ============================================================================

#[test]
fn component_with_table() {
    html_opts_i(
        concat!(
            "<.wrapper>\n",
            "\n",
            "| a | b |\n",
            "|---|---|\n",
            "| c | d |\n",
            "\n",
            "</.wrapper>\n",
        ),
        concat!(
            "<.wrapper>\n",
            "\n",
            "| a | b |\n",
            "|---|---|\n",
            "| c | d |\n",
            "\n",
            "</.wrapper>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.extension.table = true;
            opts.render.r#unsafe = true;
        },
    );
}

#[test]
fn directive_with_footnote() {
    html_opts_i(
        concat!("Text <%= @foo %>[^1]\n", "\n", "[^1]: note\n",),
        concat!(
            "<p>Text <%= @foo %><sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\" data-footnote-ref>1</a></sup></p>\n",
            "<section class=\"footnotes\" data-footnotes>\n",
            "<ol>\n",
            "<li id=\"fn-1\">\n",
            "<p>note <a href=\"#fnref-1\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"1\" aria-label=\"Back to reference 1\">↩</a></p>\n",
            "</li>\n",
            "</ol>\n",
            "</section>\n",
        ),
        true,
        |opts| {
            opts.extension.phoenix_heex = true;
            opts.extension.footnotes = true;
            opts.render.r#unsafe = true;
        },
    );
}

// ============================================================================
// edge cases - special characters and escaping
// ============================================================================

#[test]
fn directive_with_escaped_percent() {
    html_opts!(
        [extension.phoenix_heex],
        "<%= \"100%% complete\" %>\n",
        "<%= \"100%% complete\" %>\n",
    );
}

#[test]
fn directive_with_angle_brackets_in_string() {
    html_opts!(
        [extension.phoenix_heex],
        "<%= \"<tag>\" %>\n",
        "<%= \"<tag>\" %>\n",
    );
}

#[test]
fn component_with_special_chars_in_attributes() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("<.button data-key=\"<>&\\\"\">\n", "</.button>\n",),
        concat!("<.button data-key=\"<>&\\\"\">\n", "</.button>\n",),
    );
}

// ============================================================================
// edge cases - performance and limits
// ============================================================================

#[test]
fn large_component_content() {
    let content = "line of content\n".repeat(1000);
    let input = format!("<.wrapper>\n{}</.wrapper>\n", content);
    let expected = format!("<.wrapper>\n{}</.wrapper>\n", content);

    html_opts!([extension.phoenix_heex], &input, &expected,);
}

// ============================================================================
// block-level expressions
// ============================================================================

#[test]
fn block_level_expression_with_string_concatenation() {
    html_opts!(
        [extension.phoenix_heex],
        concat!("{\n", "  \"a\"\n", "  <>\n", "  \"b\"\n", "}\n",),
        concat!("{\n", "  \"a\"\n", "  <>\n", "  \"b\"\n", "}\n",),
    );
}
