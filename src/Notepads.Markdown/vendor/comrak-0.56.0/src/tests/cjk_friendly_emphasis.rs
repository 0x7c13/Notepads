use super::*;

#[test]
fn cjk_friendly_emphasis_common() {
    html_opts!(
        [extension.cjk_friendly_emphasis],
        concat!(
            "これは**私のやりたかったこと。**だからするの。\n\n",
            "**[製品ほげ](./product-foo)**と**[製品ふが](./product-bar)**をお試しください\n\n",
            "先頭の**`コード`も注意。**",
            "\n\n",
            r"**末尾の`コード`**も注意。",
            "\n\n",
            "税込**¥10,000**で入手できます。\n\n",
            "正解は**④**です。\n\n",
            "太郎は**「こんにちわ」**といった\n\n",
            r#"太郎は**"こんにちわ"**といった"#,
            "\n\n",
            "**C#**や**F#**は**「.NET」**というプラットフォーム上で動作します。\n\n",
            "Go**「初心者」**を対象とした記事です。\n\n",
            "・**㋐**:選択肢１つ目\n\n",
            ".NET**（.NET Frameworkは不可）**では、\n\n",
            "大塚︀**(U+585A U+FE00)** 大塚**(U+FA10)**\n\n",
            "〽︎**(庵点)**は、\n\n",
            "**true。︁**false\n\n",
            "禰󠄀**(ね)**豆子",
        ),
        concat!(
            "<p>これは<strong>私のやりたかったこと。</strong>だからするの。</p>\n",
            "<p><strong><a href=\"./product-foo\">製品ほげ</a></strong>と<strong><a href=\"./product-bar\">製品ふが</a></strong>をお試しください</p>\n",
            "<p>先頭の<strong><code>コード</code>も注意。</strong></p>\n",
            "<p><strong>末尾の<code>コード</code></strong>も注意。</p>\n",
            "<p>税込<strong>¥10,000</strong>で入手できます。</p>\n",
            "<p>正解は<strong>④</strong>です。</p>\n",
            "<p>太郎は<strong>「こんにちわ」</strong>といった</p>\n",
            "<p>太郎は<strong>&quot;こんにちわ&quot;</strong>といった</p>\n",
            "<p><strong>C#</strong>や<strong>F#</strong>は<strong>「.NET」</strong>というプラットフォーム上で動作します。</p>\n",
            "<p>Go<strong>「初心者」</strong>を対象とした記事です。</p>\n",
            "<p>・<strong>㋐</strong>:選択肢１つ目</p>\n",
            "<p>.NET<strong>（.NET Frameworkは不可）</strong>では、</p>\n",
            "<p>大塚︀<strong>(U+585A U+FE00)</strong> 大塚<strong>(U+FA10)</strong></p>\n",
            "<p>〽︎<strong>(庵点)</strong>は、</p>\n",
            "<p><strong>true。︁</strong>false</p>\n",
            "<p>禰󠄀<strong>(ね)</strong>豆子</p>\n",
        )
    );
}

#[test]
fn cjk_friendly_emphasis_korean() {
    html_opts!(
        [extension.cjk_friendly_emphasis],
        concat!(
            "**이 [링크](https://example.kr/)**만을 강조하고 싶다.\n\n",
            "**스크립트(script)**라고\n\n",
            "패키지를 발행하려면 **`npm publish`**를 실행하십시오.\n\n",
            "**안녕(hello)**하세요.\n\n",
            "ᅡ**(a)**\n\n",
            "**(k)**ᄏ\n\n",
        ),
        concat!(
            "<p><strong>이 <a href=\"https://example.kr/\">링크</a></strong>만을 강조하고 싶다.</p>\n",
            "<p><strong>스크립트(script)</strong>라고</p>\n",
            "<p>패키지를 발행하려면 <strong><code>npm publish</code></strong>를 실행하십시오.</p>\n",
            "<p><strong>안녕(hello)</strong>하세요.</p>\n",
            "<p>ᅡ<strong>(a)</strong></p>\n",
            "<p><strong>(k)</strong>ᄏ</p>\n",
        )
    );
}

#[test]
fn cjk_friendly_emphasis_underscore() {
    html_opts!(
        [extension.cjk_friendly_emphasis],
        concat!(
            "__注意__：注意事項\n\n",
            "注意：__注意事項__\n\n",
            "正體字。︁_Traditional._\n\n",
            "正體字。︁__Hong Kong and Taiwan.__\n\n",
            "简体字 / 新字体。︀_Simplified._\n\n",
            "简体字 / 新字体。︀__Mainland China or Japan.__\n\n",
            "“︁Git”︁__Hub__\n\n"
        ),
        concat!(
            "<p><strong>注意</strong>：注意事項</p>\n",
            "<p>注意：<strong>注意事項</strong></p>\n",
            "<p>正體字。︁<em>Traditional.</em></p>\n",
            "<p>正體字。︁<strong>Hong Kong and Taiwan.</strong></p>\n",
            "<p>简体字 / 新字体。︀<em>Simplified.</em></p>\n",
            "<p>简体字 / 新字体。︀<strong>Mainland China or Japan.</strong></p>\n",
            "<p>“︁Git”︁<strong>Hub</strong></p>\n"
        ),
    );
}

#[test]
fn cjk_friendly_emphasis_gfm_strikethrough() {
    html_opts!(
        [extension.cjk_friendly_emphasis, extension.strikethrough],
        concat!(
            "a~~a()~~あ\n\n",
            "あ~~()a~~a\n\n",
            "𩸽~~()a~~a\n\n",
            "a~~a()~~𩸽\n\n",
            "葛󠄀~~()a~~a\n\n",
            "羽︀~~()a~~a\n\n",
            "a~~「a~~」\n\n",
            "「~~a」~~a\n\n",
            "~~a~~：~~a~~\n\n",
            "~~日本語。︀~~English.\n\n",
            "~~“︁a”︁~~a\n\n"
        ),
        concat!(
            "<p>a<del>a()</del>あ</p>\n",
            "<p>あ<del>()a</del>a</p>\n",
            "<p>𩸽<del>()a</del>a</p>\n",
            "<p>a<del>a()</del>𩸽</p>\n",
            "<p>葛󠄀<del>()a</del>a</p>\n",
            "<p>羽︀<del>()a</del>a</p>\n",
            "<p>a<del>「a</del>」</p>\n",
            "<p>「<del>a」</del>a</p>\n",
            "<p><del>a</del>：<del>a</del></p>\n",
            "<p><del>日本語。︀</del>English.</p>\n",
            "<p><del>“︁a”︁</del>a</p>\n"
        ),
    );
}

#[test]
fn cjk_friendly_pseudo_emoji() {
    html_opts!(
        [extension.cjk_friendly_emphasis],
        concat!(
            "a**〰**a\n\n",
            "a**〽**a\n\n",
            "a**🈂**a\n\n",
            "a**🈷**a\n\n",
            "a**㊗**a\n\n",
            "a**㊙**a\n\n"
        ),
        concat!(
            "<p>a<strong>〰</strong>a</p>\n",
            "<p>a<strong>〽</strong>a</p>\n",
            "<p>a<strong>🈂</strong>a</p>\n",
            "<p>a<strong>🈷</strong>a</p>\n",
            "<p>a<strong>㊗</strong>a</p>\n",
            "<p>a<strong>㊙</strong>a</p>\n"
        ),
    );
}
