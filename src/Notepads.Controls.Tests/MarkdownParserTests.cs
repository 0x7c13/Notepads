// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.Tests
{
    using System.Collections.Generic;
    using System.Linq;
    using Notepads.Controls.Markdown;
    using Xunit;

    /// <summary>
    /// Regression tests for <see cref="MarkdownDocument.Parse(string)"/>.
    /// <para/>
    /// The editor hands the document text to the markdown preview with CR ('\r') as the
    /// only line ending (TextEditorCore.RichEditBoxDefaultLineEnding), so these tests use
    /// CR-terminated input. Every case below feeds text a user can type in Notepads.
    /// </summary>
    public class MarkdownParserTests
    {
        private const string Cr = "\r";

        private static List<MarkdownBlock> Parse(string markdown)
        {
            var document = new MarkdownDocument();
            document.Parse(markdown);
            return document.Blocks.ToList();
        }

        private static string TypeNames(IEnumerable<MarkdownBlock> blocks)
        {
            return string.Join(", ", blocks.Select(block => block.Type.ToString()));
        }

        /// <summary>
        /// Two horizontal rules in a row used to throw ArgumentOutOfRangeException
        /// ("length ('-4') must be a non-negative value") out of MarkdownDocument.Parse,
        /// which leaves the preview pane empty with no message for the user.
        /// A document made of two horizontal rules is ordinary markdown.
        /// </summary>
        [Fact]
        public void TwoHorizontalRulesInARow_DoNotThrow_AndYieldTwoRules()
        {
            var blocks = Parse("---" + Cr + "---");

            Assert.Equal("HorizontalRule, HorizontalRule", TypeNames(blocks));
        }

        [Fact]
        public void TwoHorizontalRulesFollowedByText_YieldRulesAndParagraph()
        {
            var blocks = Parse("---" + Cr + "---" + Cr + "Some text");

            Assert.Equal("HorizontalRule, HorizontalRule, Paragraph", TypeNames(blocks));
        }

        [Fact]
        public void TwoHorizontalRulesSeparatedByABlankLine_DoNotThrow()
        {
            var blocks = Parse("---" + Cr + Cr + "---");

            Assert.Equal("HorizontalRule, HorizontalRule", TypeNames(blocks));
        }

        [Fact]
        public void DocumentStartingWithTwoHorizontalRules_DoNotThrow()
        {
            var blocks = Parse("---" + Cr + "---" + Cr + "# Heading" + Cr + Cr + "body text");

            Assert.Equal("HorizontalRule, HorizontalRule, Header, Paragraph", TypeNames(blocks));
        }

        /// <summary>
        /// A horizontal rule followed by a line that looks like the start of a yaml header
        /// used to hit the same failure, because YamlHeaderBlock.Parse rewrote the caller's
        /// position even when it decided the input was not a yaml header.
        /// </summary>
        [Fact]
        public void HorizontalRuleFollowedByYamlLikeText_DoNotThrow()
        {
            var blocks = Parse("---" + Cr + "---" + Cr + "title: x" + Cr + "---" + Cr + "body");

            Assert.Equal("HorizontalRule, HorizontalRule, Header, Paragraph", TypeNames(blocks));
        }

        // ---------------------------------------------------------------------------------
        // Guards: the same code paths must keep parsing real yaml headers and single rules.
        // ---------------------------------------------------------------------------------

        [Fact]
        public void ValidYamlHeader_IsStillParsed()
        {
            var blocks = Parse("---" + Cr + "title: x" + Cr + "author: y" + Cr + "---" + Cr + "body");

            Assert.Equal("YamlHeader, Paragraph", TypeNames(blocks));

            var yaml = Assert.IsType<YamlHeaderBlock>(blocks[0]);
            Assert.Equal("x", yaml.Children["title"]);
            Assert.Equal("y", yaml.Children["author"]);
        }

        [Fact]
        public void ValidYamlHeaderFollowedByHorizontalRule_IsStillParsed()
        {
            var blocks = Parse("---" + Cr + "title: x" + Cr + "---" + Cr + "---" + Cr + "body");

            Assert.Equal("YamlHeader, HorizontalRule, Paragraph", TypeNames(blocks));
        }

        [Fact]
        public void SingleHorizontalRule_IsStillParsed()
        {
            var blocks = Parse("---");

            Assert.Equal("HorizontalRule", TypeNames(blocks));
        }

        [Fact]
        public void HorizontalRuleFollowedByText_IsStillParsed()
        {
            var blocks = Parse("---" + Cr + "text");

            Assert.Equal("HorizontalRule, Paragraph", TypeNames(blocks));
        }
    }
}
