// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock

namespace Notepads.Controls
{
    using System;
    using System.Collections.Generic;
    using ColorCode.Styling;
    using Windows.Foundation.Metadata;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Notepads.Controls.Helpers;
    using Notepads.Controls.Markdown;

    /// <summary>
    /// An efficient and extensible control that can parse and render markdown.
    /// </summary>
    public partial class MarkdownTextBlock
    {
        // SvgImageSource was introduced in Creators Update (15063)
        private static readonly bool _isSvgImageSupported = ApiInformation.IsApiContractPresent("Windows.Foundation.UniversalApiContract", 4);

        // Used to attach the URL to hyperlinks.
        private static readonly DependencyProperty HyperlinkUrlProperty =
            DependencyProperty.RegisterAttached("HyperlinkUrl", typeof(string), typeof(MarkdownTextBlock), new PropertyMetadata(null));

        // Checks if clicked image is a hyperlink or not.
        private static readonly DependencyProperty IsHyperlinkProperty =
            DependencyProperty.RegisterAttached("IsHyperLink", typeof(string), typeof(MarkdownTextBlock), new PropertyMetadata(null));

        /// <summary>
        /// Gets the dependency property for <see cref="CodeStyling"/>.
        /// </summary>
        public static readonly DependencyProperty CodeStylingProperty =
            DependencyProperty.Register(
                nameof(CodeStyling),
                typeof(StyleDictionary),
                typeof(MarkdownTextBlock),
                new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="UseSyntaxHighlighting"/>.
        /// </summary>
        public static readonly DependencyProperty UseSyntaxHighlightingProperty =
            DependencyProperty.Register(
                nameof(UseSyntaxHighlighting),
                typeof(bool),
                typeof(MarkdownTextBlock),
                new PropertyMetadata(true, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="WrapCodeBlock"/>.
        /// </summary>
        public static readonly DependencyProperty WrapCodeBlockProperty =
            DependencyProperty.Register(nameof(WrapCodeBlock), typeof(bool), typeof(MarkdownTextBlock), new PropertyMetadata(false));

        /// <summary>
        /// Gets the dependency property for <see cref="Text"/>.
        /// </summary>
        public static readonly DependencyProperty TextProperty = DependencyProperty.Register(
            nameof(Text),
            typeof(string),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(string.Empty, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="InlineCodeBackground"/>.
        /// </summary>
        public static readonly DependencyProperty InlineCodeBackgroundProperty =
            DependencyProperty.Register(
                nameof(InlineCodeBackground),
                typeof(Brush),
                typeof(MarkdownTextBlock),
                new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="InlineCodeForeground"/>.
        /// </summary>
        public static readonly DependencyProperty InlineCodeForegroundProperty =
            DependencyProperty.Register(
                nameof(InlineCodeForeground),
                typeof(Brush),
                typeof(MarkdownTextBlock),
                new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="InlineCodeBorderBrush"/>.
        /// </summary>
        public static readonly DependencyProperty InlineCodeBorderBrushProperty =
            DependencyProperty.Register(
                nameof(InlineCodeBorderBrush),
                typeof(Brush),
                typeof(MarkdownTextBlock),
                new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="IsTextSelectionEnabled"/>.
        /// </summary>
        public static readonly DependencyProperty IsTextSelectionEnabledProperty = DependencyProperty.Register(
            nameof(IsTextSelectionEnabled),
            typeof(bool),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(true, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="LinkForeground"/>.
        /// </summary>
        public static readonly DependencyProperty LinkForegroundProperty = DependencyProperty.Register(
            nameof(LinkForeground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="CodeBackground"/>.
        /// </summary>
        public static readonly DependencyProperty CodeBackgroundProperty = DependencyProperty.Register(
            nameof(CodeBackground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="CodeBorderBrush"/>.
        /// </summary>
        public static readonly DependencyProperty CodeBorderBrushProperty = DependencyProperty.Register(
            nameof(CodeBorderBrush),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="CodeForeground"/>.
        /// </summary>
        public static readonly DependencyProperty CodeForegroundProperty = DependencyProperty.Register(
            nameof(CodeForeground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="CodeFontFamily"/>.
        /// </summary>
        public static readonly DependencyProperty CodeFontFamilyProperty = DependencyProperty.Register(
            nameof(CodeFontFamily),
            typeof(FontFamily),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="InlineCodeFontFamily"/>.
        /// </summary>
        public static readonly DependencyProperty InlineCodeFontFamilyProperty = DependencyProperty.Register(
            nameof(InlineCodeFontFamily),
            typeof(FontFamily),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="EmojiFontFamily"/>.
        /// </summary>
        public static readonly DependencyProperty EmojiFontFamilyProperty = DependencyProperty.Register(
            nameof(EmojiFontFamily),
            typeof(FontFamily),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header1FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header1FontWeightProperty = DependencyProperty.Register(
            nameof(Header1FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header1Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header1ForegroundProperty = DependencyProperty.Register(
            nameof(Header1Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header2FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header2FontWeightProperty = DependencyProperty.Register(
            nameof(Header2FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header2Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header2ForegroundProperty = DependencyProperty.Register(
            nameof(Header2Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header3FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header3FontWeightProperty = DependencyProperty.Register(
            nameof(Header3FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header3Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header3ForegroundProperty = DependencyProperty.Register(
            nameof(Header3Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header4FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header4FontWeightProperty = DependencyProperty.Register(
            nameof(Header4FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header4Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header4ForegroundProperty = DependencyProperty.Register(
            nameof(Header4Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header5FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header5FontWeightProperty = DependencyProperty.Register(
            nameof(Header5FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header5Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header5ForegroundProperty = DependencyProperty.Register(
            nameof(Header5Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header6FontWeight"/>.
        /// </summary>
        public static readonly DependencyProperty Header6FontWeightProperty = DependencyProperty.Register(
            nameof(Header6FontWeight),
            typeof(FontWeight),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="Header6Foreground"/>.
        /// </summary>
        public static readonly DependencyProperty Header6ForegroundProperty = DependencyProperty.Register(
            nameof(Header6Foreground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="HorizontalRuleBrush"/>.
        /// </summary>
        public static readonly DependencyProperty HorizontalRuleBrushProperty = DependencyProperty.Register(
            nameof(HorizontalRuleBrush),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="QuoteBackground"/>.
        /// </summary>
        public static readonly DependencyProperty QuoteBackgroundProperty = DependencyProperty.Register(
            nameof(QuoteBackground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="QuoteBorderBrush"/>.
        /// </summary>
        public static readonly DependencyProperty QuoteBorderBrushProperty = DependencyProperty.Register(
            nameof(QuoteBorderBrush),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="QuoteForeground"/>.
        /// </summary>
        public static readonly DependencyProperty QuoteForegroundProperty = DependencyProperty.Register(
            nameof(QuoteForeground),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="TableBorderBrush"/>.
        /// </summary>
        public static readonly DependencyProperty TableBorderBrushProperty = DependencyProperty.Register(
            nameof(TableBorderBrush),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="YamlBorderBrush"/>.
        /// </summary>
        public static readonly DependencyProperty YamlBorderBrushProperty = DependencyProperty.Register(
            nameof(YamlBorderBrush),
            typeof(Brush),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(null, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="UriPrefix"/>.
        /// </summary>
        public static readonly DependencyProperty UriPrefixProperty = DependencyProperty.Register(
            nameof(UriPrefix),
            typeof(string),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(string.Empty, OnPropertyChangedStatic));

        /// <summary>
        /// Gets the dependency property for <see cref="UriPrefix"/>.
        /// </summary>
        public static readonly DependencyProperty SchemeListProperty = DependencyProperty.Register(
            nameof(SchemeList),
            typeof(string),
            typeof(MarkdownTextBlock),
            new PropertyMetadata(string.Empty, OnPropertyChangedStatic));

        /// <summary>
        /// Gets or sets the markdown text to display.
        /// </summary>
        public string Text
        {
            get => (string)GetValue(TextProperty);
            set => SetValue(TextProperty, value);
        }

        /// <summary>
        /// Gets or sets a value indicating whether to use Syntax Highlighting on Code.
        /// </summary>
        public bool UseSyntaxHighlighting
        {
            get => (bool)GetValue(UseSyntaxHighlightingProperty);
            set => SetValue(UseSyntaxHighlightingProperty, value);
        }

        /// <summary>
        /// Gets or sets a value indicating whether to Wrap the Code Block or use a Horizontal Scroll.
        /// </summary>
        public bool WrapCodeBlock
        {
            get => (bool)GetValue(WrapCodeBlockProperty);
            set => SetValue(WrapCodeBlockProperty, value);
        }

        /// <summary>
        /// Gets or sets the Default Code Styling for Code Blocks.
        /// </summary>
        public StyleDictionary CodeStyling
        {
            get => (StyleDictionary)GetValue(CodeStylingProperty);
            set => SetValue(CodeStylingProperty, value);
        }

        /// <summary>
        /// Gets or sets a value indicating whether text selection is enabled.
        /// </summary>
        public bool IsTextSelectionEnabled
        {
            get => (bool)GetValue(IsTextSelectionEnabledProperty);
            set => SetValue(IsTextSelectionEnabledProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render links.  If this is
        /// <c>null</c>, then Foreground is used.
        /// </summary>
        public Brush LinkForeground
        {
            get => (Brush)GetValue(LinkForegroundProperty);
            set => SetValue(LinkForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to fill the background of a code block.
        /// </summary>
        public Brush CodeBackground
        {
            get => (Brush)GetValue(CodeBackgroundProperty);
            set => SetValue(CodeBackgroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render the border fill of a code block.
        /// </summary>
        public Brush CodeBorderBrush
        {
            get => (Brush)GetValue(CodeBorderBrushProperty);
            set => SetValue(CodeBorderBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render the text inside a code block.  If this is
        /// <c>null</c>, then Foreground is used.
        /// </summary>
        public Brush CodeForeground
        {
            get => (Brush)GetValue(CodeForegroundProperty);
            set => SetValue(CodeForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font used to display code.  If this is <c>null</c>, then
        /// <see cref="FontFamily"/> is used.
        /// </summary>
        public FontFamily CodeFontFamily
        {
            get => (FontFamily)GetValue(CodeFontFamilyProperty);
            set => SetValue(CodeFontFamilyProperty, value);
        }

        /// <summary>
        /// Gets or sets the font used to display code.  If this is <c>null</c>, then
        /// <see cref="FontFamily"/> is used.
        /// </summary>
        public FontFamily InlineCodeFontFamily
        {
            get => (FontFamily)GetValue(InlineCodeFontFamilyProperty);
            set => SetValue(InlineCodeFontFamilyProperty, value);
        }

        /// <summary>
        /// Gets or sets the background brush for inline code.
        /// </summary>
        public Brush InlineCodeBackground
        {
            get => (Brush)GetValue(InlineCodeBackgroundProperty);
            set => SetValue(InlineCodeBackgroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for inline code.
        /// </summary>
        public Brush InlineCodeForeground
        {
            get => (Brush)GetValue(InlineCodeForegroundProperty);
            set => SetValue(InlineCodeForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the border brush for inline code.
        /// </summary>
        public Brush InlineCodeBorderBrush
        {
            get => (Brush)GetValue(InlineCodeBorderBrushProperty);
            set => SetValue(InlineCodeBorderBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the font used to display emojis.  If this is <c>null</c>, then
        /// Segoe UI Emoji font is used.
        /// </summary>
        public FontFamily EmojiFontFamily
        {
            get => (FontFamily)GetValue(EmojiFontFamilyProperty);
            set => SetValue(EmojiFontFamilyProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 1 headers.
        /// </summary>
        public FontWeight Header1FontWeight
        {
            get => (FontWeight)GetValue(Header1FontWeightProperty);
            set => SetValue(Header1FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 1 headers.
        /// </summary>
        public Brush Header1Foreground
        {
            get => (Brush)GetValue(Header1ForegroundProperty);
            set => SetValue(Header1ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 2 headers.
        /// </summary>
        public FontWeight Header2FontWeight
        {
            get => (FontWeight)GetValue(Header2FontWeightProperty);
            set => SetValue(Header2FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 2 headers.
        /// </summary>
        public Brush Header2Foreground
        {
            get => (Brush)GetValue(Header2ForegroundProperty);
            set => SetValue(Header2ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 3 headers.
        /// </summary>
        public FontWeight Header3FontWeight
        {
            get => (FontWeight)GetValue(Header3FontWeightProperty);
            set => SetValue(Header3FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 3 headers.
        /// </summary>
        public Brush Header3Foreground
        {
            get => (Brush)GetValue(Header3ForegroundProperty);
            set => SetValue(Header3ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 4 headers.
        /// </summary>
        public FontWeight Header4FontWeight
        {
            get => (FontWeight)GetValue(Header4FontWeightProperty);
            set => SetValue(Header4FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 4 headers.
        /// </summary>
        public Brush Header4Foreground
        {
            get => (Brush)GetValue(Header4ForegroundProperty);
            set => SetValue(Header4ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 5 headers.
        /// </summary>
        public FontWeight Header5FontWeight
        {
            get => (FontWeight)GetValue(Header5FontWeightProperty);
            set => SetValue(Header5FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 5 headers.
        /// </summary>
        public Brush Header5Foreground
        {
            get => (Brush)GetValue(Header5ForegroundProperty);
            set => SetValue(Header5ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the font weight to use for level 6 headers.
        /// </summary>
        public FontWeight Header6FontWeight
        {
            get => (FontWeight)GetValue(Header6FontWeightProperty);
            set => SetValue(Header6FontWeightProperty, value);
        }

        /// <summary>
        /// Gets or sets the foreground brush for level 6 headers.
        /// </summary>
        public Brush Header6Foreground
        {
            get => (Brush)GetValue(Header6ForegroundProperty);
            set => SetValue(Header6ForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render a horizontal rule.  If this is <c>null</c>, then
        /// <see cref="HorizontalRuleBrush"/> is used.
        /// </summary>
        public Brush HorizontalRuleBrush
        {
            get => (Brush)GetValue(HorizontalRuleBrushProperty);
            set => SetValue(HorizontalRuleBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to fill the background of a quote block.
        /// </summary>
        public Brush QuoteBackground
        {
            get => (Brush)GetValue(QuoteBackgroundProperty);
            set => SetValue(QuoteBackgroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render a quote border.  If this is <c>null</c>, then
        /// <see cref="QuoteBorderBrush"/> is used.
        /// </summary>
        public Brush QuoteBorderBrush
        {
            get => (Brush)GetValue(QuoteBorderBrushProperty);
            set => SetValue(QuoteBorderBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render the text inside a quote block.  If this is
        /// <c>null</c>, then Foreground is used.
        /// </summary>
        public Brush QuoteForeground
        {
            get => (Brush)GetValue(QuoteForegroundProperty);
            set => SetValue(QuoteForegroundProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render table borders.  If this is <c>null</c>, then
        /// <see cref="TableBorderBrush"/> is used.
        /// </summary>
        public Brush TableBorderBrush
        {
            get => (Brush)GetValue(TableBorderBrushProperty);
            set => SetValue(TableBorderBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the brush used to render yaml borders.  If this is <c>null</c>, then
        /// <see cref="TableBorderBrush"/> is used.
        /// </summary>
        public Brush YamlBorderBrush
        {
            get => (Brush)GetValue(TableBorderBrushProperty);
            set => SetValue(TableBorderBrushProperty, value);
        }

        /// <summary>
        /// Gets or sets the Prefix of Uri.
        /// </summary>
        public string UriPrefix
        {
            get => (string)GetValue(UriPrefixProperty);
            set => SetValue(UriPrefixProperty, value);
        }

        /// <summary>
        /// Gets or sets the SchemeList.
        /// </summary>
        public string SchemeList
        {
            get => (string)GetValue(SchemeListProperty);
            set => SetValue(SchemeListProperty, value);
        }

        /// <summary>
        /// Holds a list of hyperlinks we are listening to.
        /// </summary>
        private readonly List<object> _listeningHyperlinks = new List<object>();

        /// <summary>
        /// The root element for our rendering.
        /// </summary>
        private Border _rootElement;

        private bool multiClickDetectionTriggered;

        private Type renderertype = typeof(MarkdownRenderer);

        private ThemeListener themeListener;
    }
}