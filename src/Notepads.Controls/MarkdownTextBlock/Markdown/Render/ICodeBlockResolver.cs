// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock/Render

namespace Notepads.Controls.Markdown
{
    using Windows.UI.Xaml.Documents;

    /// <summary>
    /// A Parser to parse code strings into Syntax Highlighted text.
    /// </summary>
    public interface ICodeBlockResolver
    {
        /// <summary>
        /// Parses Code Block text into Rich text.
        /// </summary>
        /// <param name="inlineCollection">Block to add formatted Text to.</param>
        /// <param name="text">The raw code block text</param>
        /// <param name="codeLanguage">The language of the Code Block, as specified by ```{Language} on the first line of the block,
        /// e.g. <para/>
        /// ```C# <para/>
        /// public void Method();<para/>
        /// ```<para/>
        /// </param>
        /// <returns>Parsing was handled Successfully</returns>
        bool ParseSyntax(InlineCollection inlineCollection, string text, string codeLanguage);
    }
}