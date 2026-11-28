// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using EditorHost = Notepads.Presentation.Controls.TextEditor.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.MainPage;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.System;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    private readonly AccessibilitySettings _accessibility = new();
    private readonly UISettings _colors = new();
    private long? _fontFamilyCallback;
    private ulong _themeGeneration;
    private (ElementTheme Theme, Color Accent, string CodeFont, double FontSize)? _appliedStandardTheme;

    private void ViewThemeChanged(FrameworkElement sender, object args) { if (_active) ApplyPreviewTheme(); }
    private void EditorFontChanged(object sender, double args) { if (_active) ApplyPreviewTheme(); }
    private void EditorFontFamilyChanged(DependencyObject sender, DependencyProperty property)
    {
        if (_active && !_disposed) ApplyPreviewTheme();
    }
    private async void HighContrastChanged(AccessibilitySettings sender, object args) => await DispatchThemeAsync();
    private async void AccentChanged(object sender, Color args) => await DispatchThemeAsync();
    private async Task DispatchThemeAsync()
    {
        if (_disposed) return;
        try { await Dispatcher.CallOnUIThreadAsync(() => { if (_active && !_disposed) ApplyPreviewTheme(); }); }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    private void ApplyPreviewTheme()
    {
        var hc = _accessibility.HighContrast;
        var inputs = (Theme: ActualTheme, Accent: hc ? default : ThemeSettingsService.AppAccentColor,
            CodeFont: _editor.FontFamily.Source, FontSize: Math.Clamp(_editor.FontSize, 6, 144));
        // Standard palettes are immutable per effective theme. High contrast
        // reads system colors each time, including changes within the same theme.
        if (!hc && _appliedStandardTheme is { } applied && applied.Equals(inputs)) return;
        _appliedStandardTheme = null;
        var palette = EditorColorPalette.ForTheme(inputs.Theme, hc);
        var syntax = SyntaxColorPalette.ForTheme(inputs.Theme);
        var foreground = palette[EditorColorRole.Foreground];
        var background = palette[EditorColorRole.OpaqueBackground];
        var panelBackground = hc ? background : palette[EditorColorRole.CaretLine];
        PreviewLoading.Background = new SolidColorBrush(background);
        var selection = hc
            ? _colors.UIElementColor(UIElementType.Highlight)
            : inputs.Accent;
        // Match the source editor's opaque selection background.
        if (!hc) selection.A = 255;
        var generation = checked(++_themeGeneration);
        var theme = new MarkdownTheme
        {
            Generation = generation, Foreground = foreground, Background = background,
            LinkForeground = hc ? _colors.UIElementColor(UIElementType.Hotlight) :
                GetReadableLinkColor(inputs.Accent, foreground, background, panelBackground),
            SelectionForeground = hc ? _colors.UIElementColor(UIElementType.HighlightText) : palette[EditorColorRole.SelectionText],
            SelectionBackground = selection,
            BorderColor = hc ? foreground : palette[EditorColorRole.LineNumber],
            PanelBackground = panelBackground,
            FocusColor = selection,
            CodeCommentForeground = hc ? foreground : syntax[SyntaxColorRole.Comment],
            CodeStringForeground = hc ? foreground : syntax[SyntaxColorRole.String],
            CodeNumberForeground = hc ? foreground : syntax[SyntaxColorRole.Number],
            CodeKeywordForeground = hc ? foreground : syntax[SyntaxColorRole.Keyword],
            CodeTypeForeground = hc ? foreground : syntax[SyntaxColorRole.Type],
            CodeFunctionForeground = hc ? foreground : syntax[SyntaxColorRole.Attribute],
            FontFamily = "Segoe UI", CodeFontFamily = inputs.CodeFont,
            FontSize = inputs.FontSize, LineHeight = inputs.FontSize * 1.55, Padding = PreviewPadding
        };
        _view.SetTheme(theme);
        ApplyNoteTheme(theme);
        if (!hc && _themeGeneration == generation) _appliedStandardTheme = inputs;
    }

    internal static Color GetReadableLinkColor(Color accent, Color foreground, Color background, Color panelBackground)
    {
        static double Linear(byte channel)
        {
            double value = channel / 255.0;
            return value <= 0.04045 ? value / 12.92 : Math.Pow((value + 0.055) / 1.055, 2.4);
        }

        static double Luminance(Color color) =>
            0.2126 * Linear(color.R) + 0.7152 * Linear(color.G) + 0.0722 * Linear(color.B);

        static byte Mix(byte from, byte to, double amount) =>
            (byte)Math.Round(from + (to - from) * amount);

        static double Contrast(double first, double second) =>
            (Math.Max(first, second) + 0.05) / (Math.Min(first, second) + 0.05);

        double backgroundLuminance = Luminance(background);
        double panelOpacity = panelBackground.A / 255.0;
        double panelLuminance = Luminance(Color.FromArgb(255,
            Mix(background.R, panelBackground.R, panelOpacity),
            Mix(background.G, panelBackground.G, panelOpacity),
            Mix(background.B, panelBackground.B, panelOpacity)));
        bool Readable(Color color)
        {
            double luminance = Luminance(color);
            return Contrast(luminance, backgroundLuminance) >= 4.5 && Contrast(luminance, panelLuminance) >= 4.5;
        }

        accent.A = 255;
        if (Readable(accent)) return accent;

        // Normal preview palettes have an opaque black or white foreground.
        // Find the smallest blend that keeps the link text readable.
        var result = foreground;
        double low = 0, high = 1;
        for (int i = 0; i < 8; i++)
        {
            double amount = (low + high) * 0.5;
            var candidate = Color.FromArgb(255, Mix(accent.R, foreground.R, amount),
                Mix(accent.G, foreground.G, amount), Mix(accent.B, foreground.B, amount));
            if (Readable(candidate))
            {
                high = amount;
                result = candidate;
            }
            else
            {
                low = amount;
            }
        }

        return result;
    }

}
