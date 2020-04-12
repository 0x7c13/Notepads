// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System;
    using System.Collections.Generic;
    using System.Text;

    /// <summary>
    /// Represents a span containing plain text.
    /// </summary>
    public class TextRunInline : MarkdownInline, IInlineLeaf
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="TextRunInline"/> class.
        /// </summary>
        public TextRunInline()
            : base(MarkdownInlineType.TextRun)
        {
        }

        /// <summary>
        /// Gets or sets the text for this run.
        /// </summary>
        public string Text { get; set; }

        // A list of supported HTML entity names, along with their corresponding code points.
        private static readonly Dictionary<string, int> _entities = new Dictionary<string, int>
        {
            { "quot", 0x0022 }, // "
            { "amp", 0x0026 }, // &
            { "apos", 0x0027 }, // '
            { "lt", 0x003C }, // <
            { "gt", 0x003E }, // >
            { "nbsp", 0x00A0 }, // <space>
            { "#160", 0x00A0 }, // ?
            { "iexcl", 0x00A1 }, // ¡
            { "cent", 0x00A2 }, // ¢
            { "pound", 0x00A3 }, // £
            { "curren", 0x00A4 }, // ¤
            { "yen", 0x00A5 }, // ¥
            { "brvbar", 0x00A6 }, // ¦
            { "sect", 0x00A7 }, // §
            { "uml", 0x00A8 }, // ¨
            { "copy", 0x00A9 }, // ©
            { "ordf", 0x00AA }, // ª
            { "laquo", 0x00AB }, // «
            { "not", 0x00AC }, // ¬
            { "shy", 0x00AD }, // ?
            { "reg", 0x00AE }, // ®
            { "macr", 0x00AF }, // ¯
            { "deg", 0x00B0 }, // °
            { "plusmn", 0x00B1 }, // ±
            { "sup2", 0x00B2 }, // ²
            { "sup3", 0x00B3 }, // ³
            { "acute", 0x00B4 }, // ´
            { "micro", 0x00B5 }, // µ
            { "para", 0x00B6 }, // ¶
            { "middot", 0x00B7 }, // ·
            { "cedil", 0x00B8 }, // ¸
            { "sup1", 0x00B9 }, // ¹
            { "ordm", 0x00BA }, // º
            { "raquo", 0x00BB }, // »
            { "frac14", 0x00BC }, // ¼
            { "frac12", 0x00BD }, // ½
            { "frac34", 0x00BE }, // ¾
            { "iquest", 0x00BF }, // ¿
            { "Agrave", 0x00C0 }, // À
            { "Aacute", 0x00C1 }, // Á
            { "Acirc", 0x00C2 }, // Â
            { "Atilde", 0x00C3 }, // Ã
            { "Auml", 0x00C4 }, // Ä
            { "Aring", 0x00C5 }, // Å
            { "AElig", 0x00C6 }, // Æ
            { "Ccedil", 0x00C7 }, // Ç
            { "Egrave", 0x00C8 }, // È
            { "Eacute", 0x00C9 }, // É
            { "Ecirc", 0x00CA }, // Ê
            { "Euml", 0x00CB }, // Ë
            { "Igrave", 0x00CC }, // Ì
            { "Iacute", 0x00CD }, // Í
            { "Icirc", 0x00CE }, // Î
            { "Iuml", 0x00CF }, // Ï
            { "ETH", 0x00D0 }, // Ð
            { "Ntilde", 0x00D1 }, // Ñ
            { "Ograve", 0x00D2 }, // Ò
            { "Oacute", 0x00D3 }, // Ó
            { "Ocirc", 0x00D4 }, // Ô
            { "Otilde", 0x00D5 }, // Õ
            { "Ouml", 0x00D6 }, // Ö
            { "times", 0x00D7 }, // ×
            { "Oslash", 0x00D8 }, // Ø
            { "Ugrave", 0x00D9 }, // Ù
            { "Uacute", 0x00DA }, // Ú
            { "Ucirc", 0x00DB }, // Û
            { "Uuml", 0x00DC }, // Ü
            { "Yacute", 0x00DD }, // Ý
            { "THORN", 0x00DE }, // Þ
            { "szlig", 0x00DF }, // ß
            { "agrave", 0x00E0 }, // à
            { "aacute", 0x00E1 }, // á
            { "acirc", 0x00E2 }, // â
            { "atilde", 0x00E3 }, // ã
            { "auml", 0x00E4 }, // ä
            { "aring", 0x00E5 }, // å
            { "aelig", 0x00E6 }, // æ
            { "ccedil", 0x00E7 }, // ç
            { "egrave", 0x00E8 }, // è
            { "eacute", 0x00E9 }, // é
            { "ecirc", 0x00EA }, // ê
            { "euml", 0x00EB }, // ë
            { "igrave", 0x00EC }, // ì
            { "iacute", 0x00ED }, // í
            { "icirc", 0x00EE }, // î
            { "iuml", 0x00EF }, // ï
            { "eth", 0x00F0 }, // ð
            { "ntilde", 0x00F1 }, // ñ
            { "ograve", 0x00F2 }, // ò
            { "oacute", 0x00F3 }, // ó
            { "ocirc", 0x00F4 }, // ô
            { "otilde", 0x00F5 }, // õ
            { "ouml", 0x00F6 }, // ö
            { "divide", 0x00F7 }, // ÷
            { "oslash", 0x00F8 }, // ø
            { "ugrave", 0x00F9 }, // ù
            { "uacute", 0x00FA }, // ú
            { "ucirc", 0x00FB }, // û
            { "uuml", 0x00FC }, // ü
            { "yacute", 0x00FD }, // ý
            { "thorn", 0x00FE }, // þ
            { "yuml", 0x00FF }, // ÿ
            { "OElig", 0x0152 }, // Œ
            { "oelig", 0x0153 }, // œ
            { "Scaron", 0x0160 }, // Š
            { "scaron", 0x0161 }, // š
            { "Yuml", 0x0178 }, // Ÿ
            { "fnof", 0x0192 }, // ƒ
            { "circ", 0x02C6 }, // ˆ
            { "tilde", 0x02DC }, // ˜
            { "Alpha", 0x0391 }, // Α
            { "Beta", 0x0392 }, // Β
            { "Gamma", 0x0393 }, // Γ
            { "Delta", 0x0394 }, // Δ
            { "Epsilon", 0x0395 }, // Ε
            { "Zeta", 0x0396 }, // Ζ
            { "Eta", 0x0397 }, // Η
            { "Theta", 0x0398 }, // Θ
            { "Iota", 0x0399 }, // Ι
            { "Kappa", 0x039A }, // Κ
            { "Lambda", 0x039B }, // Λ
            { "Mu", 0x039C }, // Μ
            { "Nu", 0x039D }, // Ν
            { "Xi", 0x039E }, // Ξ
            { "Omicron", 0x039F }, // Ο
            { "Pi", 0x03A0 }, // Π
            { "Rho", 0x03A1 }, // Ρ
            { "Sigma", 0x03A3 }, // Σ
            { "Tau", 0x03A4 }, // Τ
            { "Upsilon", 0x03A5 }, // Υ
            { "Phi", 0x03A6 }, // Φ
            { "Chi", 0x03A7 }, // Χ
            { "Psi", 0x03A8 }, // Ψ
            { "Omega", 0x03A9 }, // Ω
            { "alpha", 0x03B1 }, // α
            { "beta", 0x03B2 }, // β
            { "gamma", 0x03B3 }, // γ
            { "delta", 0x03B4 }, // δ
            { "epsilon", 0x03B5 }, // ε
            { "zeta", 0x03B6 }, // ζ
            { "eta", 0x03B7 }, // η
            { "theta", 0x03B8 }, // θ
            { "iota", 0x03B9 }, // ι
            { "kappa", 0x03BA }, // κ
            { "lambda", 0x03BB }, // λ
            { "mu", 0x03BC }, // μ
            { "nu", 0x03BD }, // ν
            { "xi", 0x03BE }, // ξ
            { "omicron", 0x03BF }, // ο
            { "pi", 0x03C0 }, // π
            { "rho", 0x03C1 }, // ρ
            { "sigmaf", 0x03C2 }, // ς
            { "sigma", 0x03C3 }, // σ
            { "tau", 0x03C4 }, // τ
            { "upsilon", 0x03C5 }, // υ
            { "phi", 0x03C6 }, // φ
            { "chi", 0x03C7 }, // χ
            { "psi", 0x03C8 }, // ψ
            { "omega", 0x03C9 }, // ω
            { "thetasym", 0x03D1 }, // ϑ
            { "upsih", 0x03D2 }, // ϒ
            { "piv", 0x03D6 }, // ϖ
            { "ensp", 0x2002 }, //  ?
            { "emsp", 0x2003 }, //  ?
            { "thinsp", 0x2009 }, //  ?
            { "zwnj", 0x200C }, //  ?
            { "zwj", 0x200D }, //  ?
            { "lrm", 0x200E }, //  ?
            { "rlm", 0x200F }, //  ?
            { "ndash", 0x2013 }, // –
            { "mdash", 0x2014 }, // —
            { "lsquo", 0x2018 }, // ‘
            { "rsquo", 0x2019 }, // ’
            { "sbquo", 0x201A }, // ‚
            { "ldquo", 0x201C }, // “
            { "rdquo", 0x201D }, // ”
            { "bdquo", 0x201E }, // „
            { "dagger", 0x2020 }, // †
            { "Dagger", 0x2021 }, // ‡
            { "bull", 0x2022 }, // •
            { "hellip", 0x2026 }, // …
            { "permil", 0x2030 }, // ‰
            { "prime", 0x2032 }, // ′
            { "Prime", 0x2033 }, // ″
            { "lsaquo", 0x2039 }, // ‹
            { "rsaquo", 0x203A }, // ›
            { "oline", 0x203E }, // ‾
            { "frasl", 0x2044 }, // ⁄
            { "euro", 0x20AC }, // €
            { "image", 0x2111 }, // ℑ
            { "weierp", 0x2118 }, // ℘
            { "real", 0x211C }, // ℜ
            { "trade", 0x2122 }, // ™
            { "alefsym", 0x2135 }, // ℵ
            { "larr", 0x2190 }, // ←
            { "uarr", 0x2191 }, // ↑
            { "rarr", 0x2192 }, // →
            { "darr", 0x2193 }, // ↓
            { "harr", 0x2194 }, // ↔
            { "crarr", 0x21B5 }, // ↵
            { "lArr", 0x21D0 }, // ⇐
            { "uArr", 0x21D1 }, // ⇑
            { "rArr", 0x21D2 }, // ⇒
            { "dArr", 0x21D3 }, // ⇓
            { "hArr", 0x21D4 }, // ⇔
            { "forall", 0x2200 }, // ∀
            { "part", 0x2202 }, // ∂
            { "exist", 0x2203 }, // ∃
            { "empty", 0x2205 }, // ∅
            { "nabla", 0x2207 }, // ∇
            { "isin", 0x2208 }, // ∈
            { "notin", 0x2209 }, // ∉
            { "ni", 0x220B }, // ∋
            { "prod", 0x220F }, // ∏
            { "sum", 0x2211 }, // ∑
            { "minus", 0x2212 }, // −
            { "lowast", 0x2217 }, // ∗
            { "radic", 0x221A }, // √
            { "prop", 0x221D }, // ∝
            { "infin", 0x221E }, // ∞
            { "ang", 0x2220 }, // ∠
            { "and", 0x2227 }, // ∧
            { "or", 0x2228 }, // ∨
            { "cap", 0x2229 }, // ∩
            { "cup", 0x222A }, // ∪
            { "int", 0x222B }, // ∫
            { "there4", 0x2234 }, // ∴
            { "sim", 0x223C }, // ∼
            { "cong", 0x2245 }, // ≅
            { "asymp", 0x2248 }, // ≈
            { "ne", 0x2260 }, // ≠
            { "equiv", 0x2261 }, // ≡
            { "le", 0x2264 }, // ≤
            { "ge", 0x2265 }, // ≥
            { "sub", 0x2282 }, // ⊂
            { "sup", 0x2283 }, // ⊃
            { "nsub", 0x2284 }, // ⊄
            { "sube", 0x2286 }, // ⊆
            { "supe", 0x2287 }, // ⊇
            { "oplus", 0x2295 }, // ⊕
            { "otimes", 0x2297 }, // ⊗
            { "perp", 0x22A5 }, // ⊥
            { "sdot", 0x22C5 }, // ⋅
            { "lceil", 0x2308 }, // ⌈
            { "rceil", 0x2309 }, // ⌉
            { "lfloor", 0x230A }, // ⌊
            { "rfloor", 0x230B }, // ⌋
            { "lang", 0x2329 }, // 〈
            { "rang", 0x232A }, // 〉
            { "loz", 0x25CA }, // ◊
            { "spades", 0x2660 }, // ♠
            { "clubs", 0x2663 }, // ♣
            { "hearts", 0x2665 }, // ♥
            { "diams", 0x2666 }, // ♦
        };

        // A list of characters that can be escaped.
        private static readonly char[] _escapeCharacters = new char[] { '\\', '`', '*', '_', '{', '}', '[', ']', '(', ')', '#', '+', '-', '.', '!', '|', '~', '^', '&', ':', '<', '>', '/' };

        /// <summary>
        /// Parses unformatted text.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="end"> The location to stop parsing. </param>
        /// <returns> A parsed text span. </returns>
        internal static TextRunInline Parse(string markdown, int start, int end)
        {
            // Handle escape sequences and entities.
            // Note: this code is designed to be as fast as possible in the case where there are no
            // escape sequences and no entities (expected to be the common case).
            StringBuilder result = null;
            int textPos = start;
            int searchPos = start;
            while (searchPos < end)
            {
                // Look for the next backslash.
                int sequenceStartIndex = markdown.IndexOfAny(new char[] { '\\', '&' }, searchPos, end - searchPos);
                if (sequenceStartIndex == -1)
                {
                    break;
                }

                searchPos = sequenceStartIndex + 1;

                char decodedChar;
                if (markdown[sequenceStartIndex] == '\\')
                {
                    // This is an escape sequence, with one more character expected.
                    if (sequenceStartIndex >= end - 1)
                    {
                        break;
                    }

                    // Check if the character after the backslash can be escaped.
                    decodedChar = markdown[sequenceStartIndex + 1];
                    if (Array.IndexOf(_escapeCharacters, decodedChar) < 0)
                    {
                        // This character cannot be escaped.
                        continue;
                    }

                    // This here's an escape sequence!
                    if (result == null)
                    {
                        result = new StringBuilder(end - start);
                    }

                    result.Append(markdown.Substring(textPos, sequenceStartIndex - textPos));
                    result.Append(decodedChar);
                    searchPos = textPos = sequenceStartIndex + 2;
                }
                else if (markdown[sequenceStartIndex] == '&')
                {
                    // This is an entity e.g. "&nbsp;".

                    // Look for the semicolon.
                    int semicolonIndex = markdown.IndexOf(';', sequenceStartIndex + 1, end - (sequenceStartIndex + 1));

                    // Unterminated entity.
                    if (semicolonIndex == -1)
                    {
                        continue;
                    }

                    // Okay, we have an entity, but is it one we recognise?
                    string entityName = markdown.Substring(sequenceStartIndex + 1, semicolonIndex - (sequenceStartIndex + 1));

                    // Unrecognised entity.
                    if (_entities.ContainsKey(entityName) == false)
                    {
                        continue;
                    }

                    // This here's an escape sequence!
                    if (result == null)
                    {
                        result = new StringBuilder(end - start);
                    }

                    result.Append(markdown.Substring(textPos, sequenceStartIndex - textPos));
                    result.Append((char)_entities[entityName]);
                    searchPos = textPos = semicolonIndex + 1;
                }
            }

            if (result != null)
            {
                result.Append(markdown.Substring(textPos, end - textPos));
                return new TextRunInline { Text = result.ToString() };
            }

            return new TextRunInline { Text = markdown.Substring(start, end - start) };
        }

        /// <summary>
        /// Parses unformatted text.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="end"> The location to stop parsing. </param>
        /// <returns> A parsed text span. </returns>
        internal static string ResolveEscapeSequences(string markdown, int start, int end)
        {
            // Handle escape sequences only.
            // Note: this code is designed to be as fast as possible in the case where there are no
            // escape sequences (expected to be the common case).
            StringBuilder result = null;
            int textPos = start;
            int searchPos = start;
            while (searchPos < end)
            {
                // Look for the next backslash.
                int sequenceStartIndex = markdown.IndexOf('\\', searchPos, end - searchPos);
                if (sequenceStartIndex == -1)
                {
                    break;
                }

                searchPos = sequenceStartIndex + 1;

                // This is an escape sequence, with one more character expected.
                if (sequenceStartIndex >= end - 1)
                {
                    break;
                }

                // Check if the character after the backslash can be escaped.
                char decodedChar = markdown[sequenceStartIndex + 1];
                if (Array.IndexOf(_escapeCharacters, decodedChar) < 0)
                {
                    // This character cannot be escaped.
                    continue;
                }

                // This here's an escape sequence!
                if (result == null)
                {
                    result = new StringBuilder(end - start);
                }

                result.Append(markdown.Substring(textPos, sequenceStartIndex - textPos));
                result.Append(decodedChar);
                searchPos = textPos = sequenceStartIndex + 2;
            }

            if (result != null)
            {
                result.Append(markdown.Substring(textPos, end - textPos));
                return result.ToString();
            }

            return markdown.Substring(start, end - start);
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            if (Text == null)
            {
                return base.ToString();
            }

            return Text;
        }
    }
}