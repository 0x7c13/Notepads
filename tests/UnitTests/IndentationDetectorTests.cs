// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text;
using Notepads.Features.Documents.Text;

namespace Notepads.UnitTests;

public sealed class IndentationDetectorTests
{
    [Theory]
    [InlineData(-1, "class A\n{\n\tvoid F()\n\t{\n\t\treturn;\n\t}\n}\n")]
    [InlineData(2, "{\n  \"a\": {\n    \"b\": [\n      1\n    ]\n  }\n}\n")]
    [InlineData(4, "{\n    \"a\": {\n        \"b\": [\n            1\n        ]\n    }\n}\n")]
    [InlineData(4, "namespace N\n{\n    /// <summary>\n    /// Doc.\n    /// </summary>\n    public class C\n    {\n        /// <summary>Doc.</summary>\n        public void F()\n        {\n            return;\n        }\n    }\n}\n")]
    [InlineData(2, "/**\n * Top.\n */\nfunction f() {\n  /**\n   * Inner.\n   */\n  return {\n    a: 1\n  };\n}\n")]
    [InlineData(2, "a:\n  b:\n    c:\n      d:\n        e: 1\nf:\n  g:\n    h:\n      i:\n        j: 2\nk: 3\n")]
    [InlineData(-1, "a\n\tb\n\t\tc\n  d\n\te\n    f\n")]
    [InlineData(0, "a\nb\n\nc\n")]
    [InlineData(0, "  a\n   b\n    c\n   d\n")]
    [InlineData(0, "a\n\tb\nc\n  d\n")]
    [InlineData(4, "a\n    b\n        c\n    d\n  e\n")]
    [InlineData(2, "a\n    b\n        c\n      d\n")]
    public void RecognizesDominantIndentationWithoutTreatingCommentsAsCode(int expected, string text) =>
        Assert.Equal(expected, IndentationDetector.Detect(text));

    [Theory]
    [InlineData("\n")]
    [InlineData("\r\n")]
    [InlineData("\r")]
    public void AllLineEndingsProduceTheSameIndentation(string ending)
    {
        const string text = "if x:\n    y = 1\n    if z:\n        w = 2\n";
        Assert.Equal(4, IndentationDetector.Detect(text.Replace("\n", ending, StringComparison.Ordinal)));
    }

    [Fact]
    public void SamplingStopsAtTheLineBoundAndCountsCrLfOnce()
    {
        var prefix = new StringBuilder();
        for (var line = 0; line < IndentationDetector.SampleLines - 2; line++) prefix.Append("x\r\n");
        Assert.Equal(4, IndentationDetector.Detect(prefix + "a\r\n    b\r\n"));
        Assert.Equal(0, IndentationDetector.Detect(prefix + "x\r\nx\r\na\r\n    b\r\n"));
    }
}
