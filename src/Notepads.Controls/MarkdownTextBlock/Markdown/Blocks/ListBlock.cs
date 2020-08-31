// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Blocks

namespace Notepads.Controls.Markdown
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Text.RegularExpressions;

    /// <summary>
    /// Represents a list, with each list item proceeded by either a number or a bullet.
    /// </summary>
    public class ListBlock : MarkdownBlock
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="ListBlock"/> class.
        /// </summary>
        public ListBlock()
            : base(MarkdownBlockType.List)
        {
        }

        /// <summary>
        /// Gets or sets the list items.
        /// </summary>
        public IList<ListItemBlock> Items { get; set; }

        /// <summary>
        /// Gets or sets the style of the list, either numbered or bulleted.
        /// </summary>
        public ListStyle Style { get; set; }

        /// <summary>
        /// Parses a list block.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location of the first character in the block. </param>
        /// <param name="maxEnd"> The location to stop parsing. </param>
        /// <param name="quoteDepth"> The current nesting level for block quoting. </param>
        /// <param name="actualEnd"> Set to the end of the block when the return value is non-null. </param>
        /// <returns> A parsed list block, or <c>null</c> if this is not a list block. </returns>
        internal static ListBlock Parse(string markdown, int start, int maxEnd, int quoteDepth, out int actualEnd)
        {
            var russianDolls = new List<NestedListInfo>();
            int russianDollIndex = -1;
            bool previousLineWasBlank = false;
            bool inCodeBlock = false;
            ListItemBlock currentListItem = null;
            actualEnd = start;

            foreach (var lineInfo in Common.ParseLines(markdown, start, maxEnd, quoteDepth))
            {
                // Is this line blank?
                if (lineInfo.IsLineBlank)
                {
                    // The line is blank, which means the next line which contains text may end the list (or it may not...).
                    previousLineWasBlank = true;
                }
                else
                {
                    // Does the line contain a list item?
                    ListItemPreamble listItemPreamble = null;
                    if (lineInfo.FirstNonWhitespaceChar - lineInfo.StartOfLine < (russianDollIndex + 2) * 4)
                    {
                        listItemPreamble = ParseItemPreamble(markdown, lineInfo.FirstNonWhitespaceChar, lineInfo.EndOfLine);
                    }

                    if (listItemPreamble != null)
                    {
                        // Yes, this line contains a list item.

                        // Determining the nesting level is done as follows:
                        // 1. If this is the first line, then the list is not nested.
                        // 2. If the number of spaces at the start of the line is equal to that of
                        //    an existing list, then the nesting level is the same as that list.
                        // 3. Otherwise, if the number of spaces is 0-4, then the nesting level
                        //    is one level deep.
                        // 4. Otherwise, if the number of spaces is 5-8, then the nesting level
                        //    is two levels deep (but no deeper than one level more than the
                        //    previous list item).
                        // 5. Etcetera.
                        ListBlock listToAddTo = null;
                        int spaceCount = lineInfo.FirstNonWhitespaceChar - lineInfo.StartOfLine;
                        russianDollIndex = russianDolls.FindIndex(rd => rd.SpaceCount == spaceCount);
                        if (russianDollIndex >= 0)
                        {
                            // Add the new list item to an existing list.
                            listToAddTo = russianDolls[russianDollIndex].List;

                            // Don't add new list items to items higher up in the list.
                            russianDolls.RemoveRange(russianDollIndex + 1, russianDolls.Count - (russianDollIndex + 1));
                        }
                        else
                        {
                            russianDollIndex = Math.Max(1, 1 + ((spaceCount - 1) / 4));
                            if (russianDollIndex < russianDolls.Count)
                            {
                                // Add the new list item to an existing list.
                                listToAddTo = russianDolls[russianDollIndex].List;

                                // Don't add new list items to items higher up in the list.
                                russianDolls.RemoveRange(russianDollIndex + 1, russianDolls.Count - (russianDollIndex + 1));
                            }
                            else
                            {
                                // Create a new list.
                                listToAddTo = new ListBlock { Style = listItemPreamble.Style, Items = new List<ListItemBlock>() };
                                if (russianDolls.Count > 0)
                                {
                                    currentListItem.Blocks.Add(listToAddTo);
                                }

                                russianDollIndex = russianDolls.Count;
                                russianDolls.Add(new NestedListInfo { List = listToAddTo, SpaceCount = spaceCount });
                            }
                        }

                        // Add a new list item.
                        currentListItem = new ListItemBlock() { Blocks = new List<MarkdownBlock>() };
                        listToAddTo.Items.Add(currentListItem);

                        // Add the rest of the line to the builder.
                        AppendTextToListItem(currentListItem, markdown, listItemPreamble.ContentStartPos, lineInfo.EndOfLine);
                    }
                    else
                    {
                        // No, this line contains text.

                        // Is there even a list in progress?
                        if (currentListItem == null)
                        {
                            actualEnd = start;
                            return null;
                        }

                        // This is the start of a new paragraph.
                        int spaceCount = lineInfo.FirstNonWhitespaceChar - lineInfo.StartOfLine;
                        if (spaceCount == 0)
                        {
                            break;
                        }

                        russianDollIndex = Math.Min(russianDollIndex, (spaceCount - 1) / 4);
                        int lineStart = Math.Min(lineInfo.FirstNonWhitespaceChar, lineInfo.StartOfLine + ((russianDollIndex + 1) * 4));

                        // 0 spaces = end of the list.
                        // 1-4 spaces = first level.
                        // 5-8 spaces = second level, etc.
                        if (previousLineWasBlank)
                        {
                            ListBlock listToAddTo = russianDolls[russianDollIndex].List;
                            currentListItem = listToAddTo.Items[listToAddTo.Items.Count - 1];

                            ListItemBuilder builder;

                            // Prevents new Block creation if still in a Code Block.
                            if (!inCodeBlock)
                            {
                                builder = new ListItemBuilder();
                                currentListItem.Blocks.Add(builder);
                            }
                            else
                            {
                                // This can only ever be a ListItemBuilder, so it is not a null reference.
                                builder = currentListItem.Blocks.Last() as ListItemBuilder;

                                // Make up for the escaped NewLines.
                                builder.Builder.AppendLine();
                                builder.Builder.AppendLine();
                            }

                            AppendTextToListItem(currentListItem, markdown, lineStart, lineInfo.EndOfLine);
                        }
                        else
                        {
                            // Inline text. Ignores the 4 spaces that are used to continue the list.
                            AppendTextToListItem(currentListItem, markdown, lineStart, lineInfo.EndOfLine, true);
                        }
                    }

                    // Check for Closing Code Blocks.
                    if (currentListItem.Blocks.Last() is ListItemBuilder currentBlock)
                    {
                        var blockmatchcount = Regex.Matches(currentBlock.Builder.ToString(), "```").Count;
                        if (blockmatchcount > 0 && blockmatchcount % 2 != 0)
                        {
                            inCodeBlock = true;
                        }
                        else
                        {
                            inCodeBlock = false;
                        }
                    }

                    // The line was not blank.
                    previousLineWasBlank = false;
                }

                // Go to the next line.
                actualEnd = lineInfo.EndOfLine;
            }

            var result = russianDolls[0].List;
            ReplaceStringBuilders(result);
            return result;
        }

        /// <summary>
        /// Parsing helper method.
        /// </summary>
        /// <returns>Returns a ListItemPreamble</returns>
        private static ListItemPreamble ParseItemPreamble(string markdown, int start, int maxEnd)
        {
            // There are two types of lists.
            // A numbered list starts with a number, then a period ('.'), then a space.
            // A bulleted list starts with a star ('*'), dash ('-') or plus ('+'), then a period, then a space.
            ListStyle style;
            if (markdown[start] == '*' || markdown[start] == '-' || markdown[start] == '+')
            {
                style = ListStyle.Bulleted;
                start++;
            }
            else if (markdown[start] >= '0' && markdown[start] <= '9')
            {
                style = ListStyle.Numbered;
                start++;

                // Skip any other digits.
                while (start < maxEnd)
                {
                    char c = markdown[start];
                    if (c < '0' || c > '9')
                    {
                        break;
                    }

                    start++;
                }

                // Next should be a period ('.').
                if (start == maxEnd || markdown[start] != '.')
                {
                    return null;
                }

                start++;
            }
            else
            {
                return null;
            }

            // Next should be a space.
            if (start == maxEnd || (markdown[start] != ' ' && markdown[start] != '\t'))
            {
                return null;
            }

            start++;

            // This is a valid list item.
            return new ListItemPreamble { Style = style, ContentStartPos = start };
        }

        /// <summary>
        /// Parsing helper method.
        /// </summary>
        private static void AppendTextToListItem(ListItemBlock listItem, string markdown, int start, int end, bool newLine = false)
        {
            ListItemBuilder listItemBuilder = null;
            if (listItem.Blocks.Count > 0)
            {
                listItemBuilder = listItem.Blocks[listItem.Blocks.Count - 1] as ListItemBuilder;
            }

            if (listItemBuilder == null)
            {
                // Add a new block.
                listItemBuilder = new ListItemBuilder();
                listItem.Blocks.Add(listItemBuilder);
            }

            var builder = listItemBuilder.Builder;
            if (builder.Length >= 2 &&
                ParseHelpers.IsMarkdownWhiteSpace(builder[builder.Length - 2]) &&
                ParseHelpers.IsMarkdownWhiteSpace(builder[builder.Length - 1]))
            {
                builder.Length -= 2;
                builder.AppendLine();
            }
            else if (builder.Length > 0)
            {
                builder.Append(' ');
            }

            if (newLine)
            {
                builder.Append(Environment.NewLine);
            }

            builder.Append(markdown.Substring(start, end - start));
        }

        /// <summary>
        /// Parsing helper.
        /// </summary>
        /// <returns> <c>true</c> if any of the list items were parsed using the block parser. </returns>
        private static bool ReplaceStringBuilders(ListBlock list)
        {
            bool usedBlockParser = false;
            foreach (var listItem in list.Items)
            {
                // Use the inline parser if there is one paragraph, use the block parser otherwise.
                var useBlockParser = listItem.Blocks.Count(block => block.Type == MarkdownBlockType.ListItemBuilder) > 1;

                // Recursively replace any child lists.
                foreach (var block in listItem.Blocks)
                {
                    if (block is ListBlock listBlock && ReplaceStringBuilders(listBlock))
                    {
                        useBlockParser = true;
                    }
                }

                // Parse the text content of the list items.
                var newBlockList = new List<MarkdownBlock>();
                foreach (var block in listItem.Blocks)
                {
                    if (block is ListItemBuilder)
                    {
                        var blockText = ((ListItemBuilder)block).Builder.ToString();
                        if (useBlockParser)
                        {
                            // Parse the list item as a series of blocks.
                            newBlockList.AddRange(MarkdownDocument.Parse(blockText, 0, blockText.Length, quoteDepth: 0, actualEnd: out var actualEnd));
                            usedBlockParser = true;
                        }
                        else
                        {
                            // Don't allow blocks.
                            var paragraph = new ParagraphBlock
                            {
                                Inlines = Common.ParseInlineChildren(blockText, 0, blockText.Length)
                            };
                            newBlockList.Add(paragraph);
                        }
                    }
                    else
                    {
                        newBlockList.Add(block);
                    }
                }

                listItem.Blocks = newBlockList;
            }

            return usedBlockParser;
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            if (Items == null)
            {
                return base.ToString();
            }

            var result = new StringBuilder();
            for (int i = 0; i < Items.Count; i++)
            {
                if (result.Length > 0)
                {
                    result.AppendLine();
                }

                switch (Style)
                {
                    case ListStyle.Bulleted:
                        result.Append("* ");
                        break;

                    case ListStyle.Numbered:
                        result.Append(i + 1);
                        result.Append(".");
                        break;
                }

                result.Append(" ");
                result.Append(string.Join("\r\n", Items[i].Blocks));
            }

            return result.ToString();
        }
    }
}