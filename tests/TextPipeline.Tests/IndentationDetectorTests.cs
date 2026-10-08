// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using Notepads.Features.Documents.Text;

namespace NotepadsEditorTests;

internal static class IndentationDetectorTests
{
    internal static void Run()
    {
        Check(-1, "class A\n{\n\tvoid F()\n\t{\n\t\treturn;\n\t}\n}\n", "tab file");
        Check(2, "{\n  \"a\": {\n    \"b\": [\n      1\n    ]\n  }\n}\n", "2-space JSON");
        Check(4, "{\n    \"a\": {\n        \"b\": [\n            1\n        ]\n    }\n}\n", "4-space JSON");
        Check(4, "namespace N\n{\n    /// <summary>\n    /// Doc.\n    /// </summary>\n    public class C\n    {\n" +
            "        /// <summary>Doc.</summary>\n        public void F()\n        {\n            return;\n        }\n    }\n}\n",
            "4-space C# with documentation comments");
        Check(2, "/**\n * Top.\n */\nfunction f() {\n  /**\n   * Inner.\n   */\n  return {\n    a: 1\n  };\n}\n",
            "JavaScript with JSDoc lines");
        Check(2, "a:\n  b:\n    c:\n      d:\n        e: 1\nf:\n  g:\n    h:\n      i:\n        j: 2\nk: 3\n",
            "2-space nesting with dedents to column 0");
        Check(-1, "a\n\tb\n\t\tc\n  d\n\te\n    f\n", "tab majority");
        Check(0, "a\nb\n\nc\n", "no indentation");
        Check(0, "  a\n   b\n    c\n   d\n", "one-space deltas");
        Check(0, "a\n\tb\nc\n  d\n", "equal tab and space lines");
        Check(4, "a\n    b\n        c\n    d\n  e\n", "two loses to four when less than half as common");
        Check(2, "a\n    b\n        c\n      d\n", "two wins over four when at least half as common");
        const string mixed = "if x:\n    y = 1\n    if z:\n        w = 2\n";
        foreach (var (ending, name) in new[] { ("\n", "LF"), ("\r\n", "CRLF"), ("\r", "CR") })
            Check(4, mixed.Replace("\n", ending, StringComparison.Ordinal), name + " line endings");

        var prefix = new StringBuilder();
        for (var line = 0; line < IndentationDetector.SampleLines - 2; line++) prefix.Append("x\r\n");
        Check(4, prefix + "a\r\n    b\r\n", "indentation inside the line bound, counting CRLF as one break");
        Check(0, prefix + "x\r\nx\r\na\r\n    b\r\n", "indentation beyond the line bound");
        Console.WriteLine("PASS: indentation detection for tabs, 2/4-space code and JSON, documentation comments, dedents, ambiguity, CR/LF/CRLF and the line bound.");
    }

    private static void Check(int expected, string sample, string name)
    {
        var actual = IndentationDetector.Detect(sample);
        if (actual != expected) throw new Exception($"Indentation {name}: expected {expected}, got {actual}.");
    }
}
