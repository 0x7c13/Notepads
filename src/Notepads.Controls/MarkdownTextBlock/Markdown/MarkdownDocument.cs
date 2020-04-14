// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown

namespace Notepads.Controls.Markdown
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;

    /// <summary>
    /// Represents a Markdown Document. <para/>
    /// Initialize an instance and call <see cref="Parse(string)"/> to parse the Raw Markdown Text.
    /// </summary>
    public class MarkdownDocument : MarkdownBlock
    {
        /// <summary>
        /// Gets a list of URL schemes.
        /// </summary>
        public static List<string> KnownSchemes { get; private set; } = new List<string>()
        {
            "http",
            "https",
            "ftp",
            "steam",
            "irc",
            "news",
            "mumble",
            "ssh",
            "ms-windows-store",
            "sip"
        };

        private Dictionary<string, LinkReferenceBlock> _references;

        /// <summary>
        /// Initializes a new instance of the <see cref="MarkdownDocument"/> class.
        /// </summary>
        public MarkdownDocument()
            : base(MarkdownBlockType.Root)
        {
        }

        /// <summary>
        /// Gets or sets the list of block elements.
        /// </summary>
        public IList<MarkdownBlock> Blocks { get; set; }

        /// <summary>
        /// Parses markdown document text.
        /// </summary>
        /// <param name="markdownText"> The markdown text. </param>
        public void Parse(string markdownText)
        {
            Blocks = Parse(markdownText, 0, markdownText.Length, quoteDepth: 0, actualEnd: out _);

            // Remove any references from the list of blocks, and add them to a dictionary.
            for (int i = Blocks.Count - 1; i >= 0; i--)
            {
                if (Blocks[i].Type == MarkdownBlockType.LinkReference)
                {
                    var reference = (LinkReferenceBlock)Blocks[i];
                    if (_references == null)
                    {
                        _references = new Dictionary<string, LinkReferenceBlock>(StringComparer.OrdinalIgnoreCase);
                    }

                    if (!_references.ContainsKey(reference.Id))
                    {
                        _references.Add(reference.Id, reference);
                    }

                    Blocks.RemoveAt(i);
                }
            }
        }

        /// <summary>
        /// Parses a markdown document.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The position to start parsing. </param>
        /// <param name="end"> The position to stop parsing. </param>
        /// <param name="quoteDepth"> The current nesting level for block quoting. </param>
        /// <param name="actualEnd"> Set to the position at which parsing ended.  This can be
        /// different from <paramref name="end"/> when the parser is being called recursively.
        /// </param>
        /// <returns> A list of parsed blocks. </returns>
        internal static List<MarkdownBlock> Parse(string markdown, int start, int end, int quoteDepth, out int actualEnd)
        {
            // We need to parse out the list of blocks.
            // Some blocks need to start on a new paragraph (code, lists and tables) while other
            // blocks can start on any line (headers, horizontal rules and quotes).
            // Text that is outside of any other block becomes a paragraph.
            var blocks = new List<MarkdownBlock>();
            int startOfLine = start;
            bool lineStartsNewParagraph = true;
            var paragraphText = new StringBuilder();

            // These are needed to parse underline-style header blocks.
            int previousRealtStartOfLine = start;
            int previousStartOfLine = start;
            int previousEndOfLine = start;

            // Go line by line.
            while (startOfLine < end)
            {
                // Find the first non-whitespace character.
                int nonSpacePos = startOfLine;
                char nonSpaceChar = '\0';
                int realStartOfLine = startOfLine;  // i.e. including quotes.
                int expectedQuotesRemaining = quoteDepth;
                while (true)
                {
                    while (nonSpacePos < end)
                    {
                        char c = markdown[nonSpacePos];
                        if (c == '\r' || c == '\n')
                        {
                            // The line is either entirely whitespace, or is empty.
                            break;
                        }

                        if (c != ' ' && c != '\t')
                        {
                            // The line has content.
                            nonSpaceChar = c;
                            break;
                        }

                        nonSpacePos++;
                    }

                    // When parsing blocks in a blockquote context, we need to count the number of
                    // quote characters ('>').  If there are less than expected AND this is the
                    // start of a new paragraph, then stop parsing.
                    if (expectedQuotesRemaining == 0)
                    {
                        break;
                    }

                    if (nonSpaceChar == '>')
                    {
                        // Expected block quote characters should be ignored.
                        expectedQuotesRemaining--;
                        nonSpacePos++;
                        nonSpaceChar = '\0';
                        startOfLine = nonSpacePos;

                        // Ignore the first space after the quote character, if there is one.
                        if (startOfLine < end && markdown[startOfLine] == ' ')
                        {
                            startOfLine++;
                            nonSpacePos++;
                        }
                    }
                    else
                    {
                        int lastIndentation = 0;
                        string lastline = null;

                        // Determines how many Quote levels were in the last line.
                        if (realStartOfLine > 0)
                        {
                            lastline = markdown.Substring(previousRealtStartOfLine, previousEndOfLine - previousRealtStartOfLine);
                            lastIndentation = lastline.Count(c => c == '>');
                        }

                        var currentEndOfLine = Common.FindNextSingleNewLine(markdown, nonSpacePos, end, out _);
                        var currentline = markdown.Substring(realStartOfLine, currentEndOfLine - realStartOfLine);
                        var currentIndentation = currentline.Count(c => c == '>');
                        var firstChar = markdown[realStartOfLine];

                        // This is a quote that doesn't start with a Quote marker, but carries on from the last line.
                        if (lastIndentation == 1)
                        {
                            if (nonSpaceChar != '\0' && firstChar != '>')
                            {
                                break;
                            }
                        }

                        // Collapse down a level of quotes if the current indentation is greater than the last indentation.
                        // Only if the last indentation is greater than 1, and the current indentation is greater than 0
                        if (lastIndentation > 1 && currentIndentation > 0 && currentIndentation < lastIndentation)
                        {
                            break;
                        }

                        // This must be the end of the blockquote.  End the current paragraph, if any.
                        actualEnd = realStartOfLine;

                        if (paragraphText.Length > 0)
                        {
                            blocks.Add(ParagraphBlock.Parse(paragraphText.ToString()));
                        }

                        return blocks;
                    }
                }

                // Find the end of the current line.
                int endOfLine = Common.FindNextSingleNewLine(markdown, nonSpacePos, end, out int startOfNextLine);

                if (nonSpaceChar == '\0')
                {
                    // The line is empty or nothing but whitespace.
                    lineStartsNewParagraph = true;

                    // End the current paragraph.
                    if (paragraphText.Length > 0)
                    {
                        blocks.Add(ParagraphBlock.Parse(paragraphText.ToString()));
                        paragraphText.Clear();
                    }
                }
                else
                {
                    // This is a header if the line starts with a hash character,
                    // or if the line starts with '-' or a '=' character and has no other characters.
                    // Or a quote if the line starts with a greater than character (optionally preceded by whitespace).
                    // Or a horizontal rule if the line contains nothing but 3 '*', '-' or '_' characters (with optional whitespace).
                    MarkdownBlock newBlockElement = null;
                    if (nonSpaceChar == '-' && nonSpacePos == startOfLine)
                    {
                        // Yaml Header
                        newBlockElement = YamlHeaderBlock.Parse(markdown, startOfLine, markdown.Length, out startOfLine);
                        if (newBlockElement != null)
                        {
                            realStartOfLine = startOfLine;
                            endOfLine = startOfLine + 3;
                            startOfNextLine = Common.FindNextSingleNewLine(markdown, startOfLine, end, out startOfNextLine);

                            paragraphText.Clear();
                        }
                    }

                    if (newBlockElement == null && nonSpaceChar == '#' && nonSpacePos == startOfLine)
                    {
                        // Hash-prefixed header.
                        newBlockElement = HeaderBlock.ParseHashPrefixedHeader(markdown, startOfLine, endOfLine);
                    }
                    else if ((nonSpaceChar == '-' || nonSpaceChar == '=') && nonSpacePos == startOfLine && paragraphText.Length > 0)
                    {
                        // Underline style header. These are weird because you don't know you've
                        // got one until you've gone past it.
                        // Note: we intentionally deviate from reddit here in that we only
                        // recognize this type of header if the previous line is part of a
                        // paragraph.  For example if you have this, the header at the bottom is
                        // ignored:
                        //   a|b
                        //   -|-
                        //   1|2
                        //   ===
                        newBlockElement = HeaderBlock.ParseUnderlineStyleHeader(markdown, previousStartOfLine, previousEndOfLine, startOfLine, endOfLine);

                        if (newBlockElement != null)
                        {
                            // We're going to have to remove the header text from the pending
                            // paragraph by prematurely ending the current paragraph.
                            // We already made sure that there is a paragraph in progress.
                            paragraphText.Length -= (previousEndOfLine - previousStartOfLine);
                        }
                    }

                    // These characters overlap with the underline-style header - this check should go after that one.
                    if (newBlockElement == null && (nonSpaceChar == '*' || nonSpaceChar == '-' || nonSpaceChar == '_'))
                    {
                        newBlockElement = HorizontalRuleBlock.Parse(markdown, startOfLine, endOfLine);
                    }

                    if (newBlockElement == null && lineStartsNewParagraph)
                    {
                        // Some block elements must start on a new paragraph (tables, lists and code).
                        int endOfBlock = startOfNextLine;
                        if (nonSpaceChar == '*' || nonSpaceChar == '+' || nonSpaceChar == '-' || (nonSpaceChar >= '0' && nonSpaceChar <= '9'))
                        {
                            newBlockElement = ListBlock.Parse(markdown, realStartOfLine, end, quoteDepth, out endOfBlock);
                        }

                        if (newBlockElement == null && (nonSpacePos > startOfLine || nonSpaceChar == '`'))
                        {
                            newBlockElement = CodeBlock.Parse(markdown, realStartOfLine, end, quoteDepth, out endOfBlock);
                        }

                        if (newBlockElement == null)
                        {
                            newBlockElement = TableBlock.Parse(markdown, realStartOfLine, endOfLine, end, quoteDepth, out endOfBlock);
                        }

                        if (newBlockElement != null)
                        {
                            startOfNextLine = endOfBlock;
                        }
                    }

                    // This check needs to go after the code block check.
                    if (newBlockElement == null && nonSpaceChar == '>')
                    {
                        newBlockElement = QuoteBlock.Parse(markdown, realStartOfLine, end, quoteDepth, out startOfNextLine);
                    }

                    // This check needs to go after the code block check.
                    if (newBlockElement == null && nonSpaceChar == '[')
                    {
                        newBlockElement = LinkReferenceBlock.Parse(markdown, startOfLine, endOfLine);
                    }

                    // Block elements start new paragraphs.
                    lineStartsNewParagraph = newBlockElement != null;

                    if (newBlockElement == null)
                    {
                        // The line contains paragraph text.
                        if (paragraphText.Length > 0)
                        {
                            // If the previous two characters were both spaces, then append a line break.
                            if (paragraphText.Length > 2 && paragraphText[paragraphText.Length - 1] == ' ' && paragraphText[paragraphText.Length - 2] == ' ')
                            {
                                // Replace the two spaces with a line break.
                                paragraphText[paragraphText.Length - 2] = '\r';
                                paragraphText[paragraphText.Length - 1] = '\n';
                            }
                            else
                            {
                                paragraphText.Append(" ");
                            }
                        }

                        // Add the last paragraph if we are at the end of the input text.
                        if (startOfNextLine >= end)
                        {
                            if (paragraphText.Length == 0)
                            {
                                // Optimize for single line paragraphs.
                                blocks.Add(ParagraphBlock.Parse(markdown.Substring(startOfLine, endOfLine - startOfLine)));
                            }
                            else
                            {
                                // Slow path.
                                paragraphText.Append(markdown.Substring(startOfLine, endOfLine - startOfLine));
                                blocks.Add(ParagraphBlock.Parse(paragraphText.ToString()));
                            }
                        }
                        else
                        {
                            paragraphText.Append(markdown.Substring(startOfLine, endOfLine - startOfLine));
                        }
                    }
                    else
                    {
                        // The line contained a block.  End the current paragraph, if any.
                        if (paragraphText.Length > 0)
                        {
                            blocks.Add(ParagraphBlock.Parse(paragraphText.ToString()));
                            paragraphText.Clear();
                        }

                        blocks.Add(newBlockElement);
                    }
                }

                // Repeat.
                previousRealtStartOfLine = realStartOfLine;
                previousStartOfLine = startOfLine;
                previousEndOfLine = endOfLine;
                startOfLine = startOfNextLine;
            }

            actualEnd = startOfLine;
            return blocks;
        }

        /// <summary>
        /// Looks up a reference using the ID.
        /// A reference is a line that looks like this:
        /// [foo]: http://example.com/
        /// </summary>
        /// <param name="id"> The ID of the reference (case insensitive). </param>
        /// <returns> The reference details, or <c>null</c> if the reference wasn't found. </returns>
        public LinkReferenceBlock LookUpReference(string id)
        {
            if (id == null)
            {
                throw new ArgumentNullException(nameof(id));
            }

            if (_references == null)
            {
                return null;
            }

            if (_references.TryGetValue(id, out LinkReferenceBlock result))
            {
                return result;
            }

            return null;
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            if (Blocks == null)
            {
                return base.ToString();
            }

            return string.Join("\r\n", Blocks);
        }
    }
}