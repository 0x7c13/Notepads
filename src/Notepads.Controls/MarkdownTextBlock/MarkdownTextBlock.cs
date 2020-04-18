// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock

namespace Notepads.Controls
{
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Notepads.Controls.Helpers;
    using Notepads.Controls.Markdown;

    /// <summary>
    /// An efficient and extensible control that can parse and render markdown.
    /// </summary>
    public partial class MarkdownTextBlock : Control, ILinkRegister, IImageResolver, ICodeBlockResolver
    {
        private long _fontSizePropertyToken;
        private long _flowDirectionPropertyToken;
        private long _backgroundPropertyToken;
        private long _borderBrushPropertyToken;
        private long _borderThicknessPropertyToken;
        private long _characterSpacingPropertyToken;
        private long _fontFamilyPropertyToken;
        private long _fontStretchPropertyToken;
        private long _fontStylePropertyToken;
        private long _fontWeightPropertyToken;
        private long _foregroundPropertyToken;
        private long _paddingPropertyToken;
        private long _requestedThemePropertyToken;

        /// <summary>
        /// Initializes a new instance of the <see cref="MarkdownTextBlock"/> class.
        /// </summary>
        public MarkdownTextBlock()
        {
            // Set our style.
            DefaultStyleKey = typeof(MarkdownTextBlock);

            Loaded += OnLoaded;
            Unloaded += OnUnloaded;
        }

        private void ThemeListener_ThemeChanged(Helpers.ThemeListener sender)
        {
            RenderMarkdown();
        }

        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            RegisterThemeChangedHandler();
            HookListeners();

            // Register for property callbacks that are owned by our parent class.
            _fontSizePropertyToken = RegisterPropertyChangedCallback(FontSizeProperty, OnPropertyChanged);
            _flowDirectionPropertyToken = RegisterPropertyChangedCallback(FlowDirectionProperty, OnPropertyChanged);
            _backgroundPropertyToken = RegisterPropertyChangedCallback(BackgroundProperty, OnPropertyChanged);
            _borderBrushPropertyToken = RegisterPropertyChangedCallback(BorderBrushProperty, OnPropertyChanged);
            _borderThicknessPropertyToken = RegisterPropertyChangedCallback(BorderThicknessProperty, OnPropertyChanged);
            _characterSpacingPropertyToken = RegisterPropertyChangedCallback(CharacterSpacingProperty, OnPropertyChanged);
            _fontFamilyPropertyToken = RegisterPropertyChangedCallback(FontFamilyProperty, OnPropertyChanged);
            _fontStretchPropertyToken = RegisterPropertyChangedCallback(FontStretchProperty, OnPropertyChanged);
            _fontStylePropertyToken = RegisterPropertyChangedCallback(FontStyleProperty, OnPropertyChanged);
            _fontWeightPropertyToken = RegisterPropertyChangedCallback(FontWeightProperty, OnPropertyChanged);
            _foregroundPropertyToken = RegisterPropertyChangedCallback(ForegroundProperty, OnPropertyChanged);
            _paddingPropertyToken = RegisterPropertyChangedCallback(PaddingProperty, OnPropertyChanged);
            _requestedThemePropertyToken = RegisterPropertyChangedCallback(RequestedThemeProperty, OnPropertyChanged);
        }

        private void OnUnloaded(object sender, RoutedEventArgs e)
        {
            if (themeListener != null)
            {
                UnhookListeners();
                themeListener.ThemeChanged -= ThemeListener_ThemeChanged;
                //themeListener.Dispose();
                //themeListener = null;
            }

            // Register for property callbacks that are owned by our parent class.
            UnregisterPropertyChangedCallback(FontSizeProperty, _fontSizePropertyToken);
            UnregisterPropertyChangedCallback(FlowDirectionProperty, _flowDirectionPropertyToken);
            UnregisterPropertyChangedCallback(BackgroundProperty, _backgroundPropertyToken);
            UnregisterPropertyChangedCallback(BorderBrushProperty, _borderBrushPropertyToken);
            UnregisterPropertyChangedCallback(BorderThicknessProperty, _borderThicknessPropertyToken);
            UnregisterPropertyChangedCallback(CharacterSpacingProperty, _characterSpacingPropertyToken);
            UnregisterPropertyChangedCallback(FontFamilyProperty, _fontFamilyPropertyToken);
            UnregisterPropertyChangedCallback(FontStretchProperty, _fontStretchPropertyToken);
            UnregisterPropertyChangedCallback(FontStyleProperty, _fontStylePropertyToken);
            UnregisterPropertyChangedCallback(FontWeightProperty, _fontWeightPropertyToken);
            UnregisterPropertyChangedCallback(ForegroundProperty, _foregroundPropertyToken);
            UnregisterPropertyChangedCallback(PaddingProperty, _paddingPropertyToken);
            UnregisterPropertyChangedCallback(RequestedThemeProperty, _requestedThemePropertyToken);
        }

        /// <inheritdoc />
        protected override void OnApplyTemplate()
        {
            RegisterThemeChangedHandler();

            // Grab our root
            _rootElement = GetTemplateChild("RootElement") as Border;

            // And make sure to render any markdown we have.
            RenderMarkdown();
        }

        private void RegisterThemeChangedHandler()
        {
            themeListener = themeListener ?? new ThemeListener();
            themeListener.ThemeChanged -= ThemeListener_ThemeChanged;
            themeListener.ThemeChanged += ThemeListener_ThemeChanged;
        }
    }
}