namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Text.RegularExpressions;
    using Windows.UI.Text;
    using Notepads.Controls.FindAndReplace;
    using Notepads.Utilities;

    public partial class TextEditorCore
    {
        public string GetSearchString()
        {
            var searchString = Document.Selection.Text.Trim();

            if (searchString.Contains(RichEditBoxDefaultLineEnding.ToString())) return string.Empty;

            if (string.IsNullOrEmpty(searchString) && Document.Selection.StartPosition < _content.Length)
            {
                var startIndex = Document.Selection.StartPosition;
                var endIndex = startIndex;

                for (; startIndex >= 0; startIndex--)
                {
                    if (!char.IsLetterOrDigit(_content[startIndex]))
                        break;
                }

                for (; endIndex < _content.Length; endIndex++)
                {
                    if (!char.IsLetterOrDigit(_content[endIndex]))
                        break;
                }

                if (startIndex != endIndex) return _content.Substring(startIndex + 1, endIndex - startIndex - 1);
            }

            return searchString;
        }

        public bool TryFindNextAndSelect(SearchContext searchContext, bool stopAtEof, out bool regexError)
        {
            regexError = false;

            if (string.IsNullOrEmpty(searchContext.SearchText))
            {
                return false;
            }

            var text = GetText();

            if (Document.Selection.EndPosition > text.Length) Document.Selection.EndPosition = text.Length;

            if (searchContext.UseRegex)
            {
                return TryFindNextAndSelectUsingRegex(text, searchContext, stopAtEof, out regexError);
            }
            else
            {
                StringComparison comparison = searchContext.MatchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

                var index = searchContext.MatchWholeWord
                    ? StringUtility.IndexOfWholeWord(text, Document.Selection.EndPosition, searchContext.SearchText, comparison)
                    : text.IndexOf(searchContext.SearchText, Document.Selection.EndPosition, comparison);

                if (index != -1)
                {
                    Document.Selection.StartPosition = index;
                    Document.Selection.EndPosition = index + searchContext.SearchText.Length;
                }
                else
                {
                    if (!stopAtEof)
                    {
                        index = searchContext.MatchWholeWord
                            ? StringUtility.IndexOfWholeWord(text, 0, searchContext.SearchText, comparison)
                            : text.IndexOf(searchContext.SearchText, 0, comparison);

                        if (index != -1)
                        {
                            Document.Selection.StartPosition = index;
                            Document.Selection.EndPosition = index + searchContext.SearchText.Length;
                        }
                    }
                }

                if (index == -1)
                {
                    Document.Selection.StartPosition = Document.Selection.EndPosition;
                    return false;
                }

                return true;
            }
        }

        public bool TryFindPreviousAndSelect(SearchContext searchContext, bool stopAtBof, out bool regexError)
        {
            regexError = false;

            if (string.IsNullOrEmpty(searchContext.SearchText))
            {
                return false;
            }

            var text = GetText();

            if (searchContext.UseRegex)
            {
                return TryFindPreviousAndSelectUsingRegex(text, searchContext, out regexError, stopAtBof);
            }
            else
            {
                var searchIndex = Document.Selection.StartPosition - 1;
                if (!stopAtBof && searchIndex < 0)
                {
                    searchIndex = text.Length - 1;
                }
                else if (stopAtBof)
                {
                    return false;
                }

                StringComparison comparison = searchContext.MatchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

                var index = searchContext.MatchWholeWord
                    ? StringUtility.LastIndexOfWholeWord(text, searchIndex, searchContext.SearchText, comparison)
                    : text.LastIndexOf(searchContext.SearchText, searchIndex, comparison);

                if (index != -1)
                {
                    Document.Selection.StartPosition = index;
                    Document.Selection.EndPosition = index + searchContext.SearchText.Length;
                }
                else
                {
                    index = searchContext.MatchWholeWord
                        ? StringUtility.LastIndexOfWholeWord(text, text.Length - 1, searchContext.SearchText, comparison)
                        : text.LastIndexOf(searchContext.SearchText, text.Length - 1, comparison);

                    if (index != -1)
                    {
                        Document.Selection.StartPosition = index;
                        Document.Selection.EndPosition = index + searchContext.SearchText.Length;
                    }
                }

                if (index == -1)
                {
                    Document.Selection.StartPosition = Document.Selection.EndPosition;
                    return false;
                }

                return true;
            }
        }

        public bool TryFindNextAndReplace(SearchContext searchContext, string replaceText, out bool regexError)
        {
            if (TryFindNextAndSelect(searchContext, stopAtEof: true, out var error))
            {
                regexError = error;
                Document.Selection.SetText(TextSetOptions.None, replaceText);
                return true;
            }

            regexError = error;
            return false;
        }

        public bool TryFindAndReplaceAll(SearchContext searchContext, string replaceText, out bool regexError)
        {
            regexError = false;
            var found = false;
            var text = GetText();

            if (string.IsNullOrEmpty(searchContext.SearchText))
            {
                return false;
            }

            if (searchContext.UseRegex)
            {
                found = TryFindAndReplaceAllUsingRegex(text, searchContext, replaceText, out regexError, out var output);
                if (found) text = output;
            }
            else
            {
                var pos = 0;
                var searchTextLength = searchContext.SearchText.Length;
                var replaceTextLength = replaceText.Length;

                StringComparison comparison = searchContext.MatchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

                pos = searchContext.MatchWholeWord
                    ? StringUtility.IndexOfWholeWord(text, pos, searchContext.SearchText, comparison)
                    : text.IndexOf(searchContext.SearchText, pos, comparison);

                while (pos != -1)
                {
                    found = true;
                    text = text.Remove(pos, searchTextLength).Insert(pos, replaceText);
                    pos += replaceTextLength;
                    pos = searchContext.MatchWholeWord
                        ? StringUtility.IndexOfWholeWord(text, pos, searchContext.SearchText, comparison)
                        : text.IndexOf(searchContext.SearchText, pos, comparison);
                }
            }

            if (found)
            {
                SetText(text);
                Document.Selection.StartPosition = int.MaxValue;
                Document.Selection.EndPosition = Document.Selection.StartPosition;
            }

            return found;
        }

        private bool TryFindNextAndSelectUsingRegex(string content, SearchContext searchContext, bool stopAtEof,
            out bool regexError)
        {
            try
            {
                regexError = false;
                Regex regex = new Regex(searchContext.SearchText,
                    RegexOptions.Compiled | (searchContext.MatchCase ? RegexOptions.None : RegexOptions.IgnoreCase));

                var match = regex.Match(content, Document.Selection.EndPosition);

                if (!match.Success && !stopAtEof)
                {
                    match = regex.Match(content, 0);
                }

                if (match.Success)
                {
                    var index = match.Index;
                    Document.Selection.StartPosition = index;
                    Document.Selection.EndPosition = index + match.Length;
                    return true;
                }
                else
                {
                    Document.Selection.StartPosition = Document.Selection.EndPosition;
                    return false;
                }
            }
            catch (Exception)
            {
                regexError = true;
                return false;
            }
        }

        private bool TryFindPreviousAndSelectUsingRegex(string content, SearchContext searchContext, out bool regexError, bool stopAtBof)
        {
            try
            {
                regexError = false;
                Regex regex = new Regex(searchContext.SearchText,
                    RegexOptions.RightToLeft | RegexOptions.Compiled |
                    (searchContext.MatchCase ? RegexOptions.None : RegexOptions.IgnoreCase));

                var match = regex.Match(content, Document.Selection.StartPosition);

                if (!match.Success && !stopAtBof)
                {
                    match = regex.Match(content, content.Length);
                }

                if (match.Success)
                {
                    var index = match.Index;
                    Document.Selection.StartPosition = index;
                    Document.Selection.EndPosition = index + match.Length;
                    return true;
                }
                else
                {
                    Document.Selection.StartPosition = Document.Selection.EndPosition;
                    return false;
                }
            }
            catch (Exception)
            {
                regexError = true;
                return false;
            }
        }

        private static bool TryFindAndReplaceAllUsingRegex(string content, SearchContext searchContext, string replaceText, out bool regexError, out string output)
        {
            regexError = false;
            output = string.Empty;

            try
            {
                Regex regex = new Regex(searchContext.SearchText,
                    RegexOptions.Compiled | (searchContext.MatchCase ? RegexOptions.None : RegexOptions.IgnoreCase));
                if (regex.IsMatch(content))
                {
                    output = regex.Replace(content, replaceText);
                    return true;
                }
            }
            catch (Exception)
            {
                regexError = true;
                return false;
            }

            return false;
        }
    }
}