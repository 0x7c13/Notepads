// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.TextEditor
{
    using Windows.UI.Text;
    using System.Text;

    public partial class TextEditorCore
    {
        public void Capitalize()
        {
            if (Document.Selection.Length != 0)
            {
                Document.Selection.ChangeCase(LetterCase.Upper);
            }
            else
            {
                Document.GetText(TextGetOptions.None, out var text);
                Document.SetText(TextSetOptions.None, text.TrimEnd().ToUpperInvariant());
            }
        }

        public void Decapitalize()
        {
            if (Document.Selection.Length != 0)
            {
                Document.Selection.ChangeCase(LetterCase.Lower);
            }
            else
            {
                Document.GetText(TextGetOptions.None, out var text);
                Document.SetText(TextSetOptions.None, text.TrimEnd().ToLowerInvariant());
            }
        }

        public void SentenceCase()
        {
            if (Document.Selection.Length != 0)
            {
                return;
            }

            Document.GetText(TextGetOptions.None, out var text);

            if (string.IsNullOrEmpty(text))
            {
                return;
            }

            var sb = new StringBuilder(text);
            bool capitalizeNext = true;

            for (int i = 0; i < sb.Length; i++)
            {
                var ch = sb[i];

                if (capitalizeNext)
                {
                    if (char.IsWhiteSpace(ch) || IsOpeningPunctuation(ch))
                    {
                        continue;
                    }

                    if (char.IsLetter(ch))
                    {
                        sb[i] = char.ToUpperInvariant(ch);
                    }
                    capitalizeNext = false;
                }
                else
                {
                    if (IsSentenceEndingPunctuation(ch))
                    {
                        capitalizeNext = true;
                    }
                }
            }

            Document.SetText(TextSetOptions.None, sb.ToString().TrimEnd());
        }

        private static bool IsSentenceEndingPunctuation(char c)
        {
            return c == '.' || c == '!' || c == '?';
        }

        private static bool IsOpeningPunctuation(char c)
        {
            switch (c)
            {
                case '"':
                case '\'':
                case '(':
                case '[':
                case '<':
                case '{':
                case '\u2018': // ‘
                case '\u2019': // ’
                case '\u201C': // “
                case '\u201D': // ”
                case '\u00AB': // «
                case '\u00BB': // »
                    return true;

                default:
                    return false;
            }
        }

        public void TitleCase()
        {
            if (Document.Selection.Length != 0)
            {
                Document.Selection.GetText(TextGetOptions.None, out var selectedText);
                var output = ToTitleCase(selectedText);
                Document.Selection.SetText(TextSetOptions.None, output);
            }
            else
            {
                Document.GetText(TextGetOptions.None, out var text);
                var output = ToTitleCase(text);
                Document.SetText(TextSetOptions.None, output.TrimEnd());
            }
        }

        private string ToTitleCase(string input)
        {
            if (string.IsNullOrEmpty(input))
            {
                return string.Empty;
            }

            var sb = new StringBuilder(input.Length);
            bool newWord = true;

            foreach (var ch in input)
            {
                if (char.IsLetter(ch))
                {
                    if (newWord)
                    {
                        sb.Append(char.ToUpperInvariant(ch));
                    }
                    else
                    {
                        sb.Append(char.ToLowerInvariant(ch));
                    }
                    newWord = false;
                }
                else
                {
                    sb.Append(ch);
                    newWord = true;
                }
            }

            return sb.ToString();
        }

        public void ToggleCase()
        {
            if (Document.Selection.Length != 0)
            {
                Document.Selection.GetText(TextGetOptions.None, out var selectedText);
                var output = ToggleCase(selectedText);
                Document.Selection.SetText(TextSetOptions.None, output);
            }
            else
            {
                Document.GetText(TextGetOptions.None, out var text);
                var output = ToggleCase(text);
                Document.SetText(TextSetOptions.None, output.TrimEnd());
            }
        }

        private string ToggleCase(string text)
        {
            if (string.IsNullOrEmpty(text))
            {
                return string.Empty;
            }
            var sb = new StringBuilder(text?.Length ?? 0);
            foreach (var ch in text)
            {
                if (char.IsUpper(ch))
                {
                    sb.Append(char.ToLowerInvariant(ch));
                }
                else if (char.IsLower(ch))
                {
                    sb.Append(char.ToUpperInvariant(ch));
                }
                else
                {
                    sb.Append(ch);
                }
            }
            return sb.ToString();
        }
    }
}