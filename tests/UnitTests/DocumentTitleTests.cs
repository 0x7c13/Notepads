// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Globalization;
using Notepads.Features.Documents.Text;

namespace Notepads.UnitTests;

public sealed class DocumentTitleTests
{
    [Theory]
    [InlineData("", null)]
    [InlineData(" \t\r\n\u00a0", null)]
    [InlineData("\0\u202e\u2066\u2069", null)]
    [InlineData("  hello\t world  \rsecond", "hello world")]
    [InlineData("\r\n \t\r\n中文标题\nnext", "中文标题")]
    [InlineData("مرحبا بالعالم\rnext", "مرحبا بالعالم")]
    [InlineData("abc\u202eDEF\u202c\0", "abcDEF")]
    [InlineData("👨‍👩‍👧‍👦 😀 👍🏽", "👨‍👩‍👧‍👦 😀 👍🏽")]
    [InlineData("e\u0301 and a\u0308", "e\u0301 and a\u0308")]
    [InlineData("---", "---")]
    [InlineData("before\u2028after", "before")]
    [InlineData("before\u0085after", "before")]
    public void FirstVisibleLineIsNormalizedWithoutChangingItsText(string text, string expected) =>
        Assert.Equal(expected, DocumentTitle.FromPrefix(text));

    [Fact]
    public void InvalidUtf16UsesAVisibleReplacementCharacter() =>
        Assert.Equal("�hello", DocumentTitle.FromPrefix(new string((char)0xd800, 1) + "hello"));

    [Fact]
    public void TruncationCountsWholeGraphemes()
    {
        var emoji = string.Concat(Enumerable.Repeat("👍🏽", 41));
        Assert.Equal(string.Concat(Enumerable.Repeat("👍🏽", 39)) + "…", DocumentTitle.FromPrefix(emoji));
        // The UTF-16 budget independently bounds unusually large clusters.
        var text = string.Concat(Enumerable.Repeat("e\u0301", 41));
        Assert.Equal(string.Concat(Enumerable.Repeat("e\u0301", 40)) + "…", DocumentTitle.FromPrefix(text));
        Assert.Equal(new string('a', 40), DocumentTitle.FromPrefix(new string('a', 40)));
        Assert.Equal(new string('a', 40) + "…", DocumentTitle.FromPrefix(new string('a', 41)));
    }

    [Fact]
    public void APartialPrefixNeverPublishesAnUnprovenFinalCluster()
    {
        Assert.Equal("hello…", DocumentTitle.FromPrefix("hello👨‍", complete: false));
        Assert.Equal("line", DocumentTitle.FromPrefix("line\nnext", complete: false));
        Assert.Null(DocumentTitle.FromPrefix("e" + new string('\u0301', 500), complete: false));
        Assert.Null(DocumentTitle.FromPrefix(new string(' ', 2048) + "outside the bounded prefix"));
    }

    [Fact]
    public void CachedTitlesStayBoundedAndStableUnderNormalization()
    {
        foreach (var text in new[] { new string('a', 2000), string.Concat(Enumerable.Repeat("👨‍👩‍👧‍👦", 100)),
            "e" + new string('\u0301', 500), "  hello \t world\nsecond" })
        {
            var title = DocumentTitle.FromPrefix(text);
            if (title == null) continue;
            Assert.InRange(title.Length, 1, DocumentTitle.MaximumLength);
            Assert.Equal(title, DocumentTitle.FromPrefix(title));
            Assert.True(new StringInfo(title).LengthInTextElements <= 41);
        }
    }
}
