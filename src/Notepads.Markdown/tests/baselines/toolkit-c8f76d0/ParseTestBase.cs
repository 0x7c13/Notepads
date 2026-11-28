// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.Text;
using Microsoft.Toolkit.Parsers.Markdown;
using Microsoft.Toolkit.Parsers.Markdown.Blocks;

namespace UnitTests.Markdown.Parse
{
    /// <summary>
    /// The base class for our display unit tests.
    /// </summary>
    public abstract class ParseTestBase : TestBase
    {
#pragma warning disable CS0618 // Type or member is obsolete
        internal void AssertEqual(string markdown, params MarkdownBlock[] expectedAst)
        {
            var expected = new StringBuilder();
            foreach (var block in expectedAst)
            {
                SerializeElement(expected, block, indentLevel: 0);
            }

            var parser = new MarkdownDocument();
            parser.Parse(markdown);

            var actual = new StringBuilder();
            foreach (var block in parser.Blocks)
            {
                SerializeElement(actual, block, indentLevel: 0);
            }

            Assert.AreEqual(expected.ToString(), actual.ToString());
        }
#pragma warning restore CS0618 // Type or member is obsolete
    }
}