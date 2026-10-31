// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.Settings;
using Windows.ApplicationModel.Resources.Core;
using Windows.Foundation;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class AppearanceFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var page = host.Page;
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        editor.TypeText("selection Ω😀\rsecond row");
        editor.TestNativeCore.SetTextSelectionPosition(0, 9);
        var native = editor.TestNativeCore.TestFeatureNative;
        var originalTheme = ThemeSettingsService.ThemeMode;
        var originalUseWindowsTheme = ThemeSettingsService.UseWindowsTheme;
        var originalAccent = ThemeSettingsService.AppAccentColor;
        var originalCustomAccent = ThemeSettingsService.CustomAccentColor;
        var originalUseWindowsAccent = ThemeSettingsService.UseWindowsAccentColor;
        var originalTint = ThemeSettingsService.AppBackgroundPanelTintOpacity;
        try
        {
            var settings = await SettingsAsync<PersonalizationSettingsPage>(page, "Personalization");
            var picker = Named<ColorPicker>(settings, "AccentColorPicker");
            Named<ToggleSwitch>(settings, "AccentColorToggle").IsOn = false;
            Check(picker.IsEnabled, "Custom accent toggle enables the production color picker.");
            picker.Color = Color.FromArgb(255, 138, 43, 226);
            var tintSlider = Named<Slider>(settings, "BackgroundTintOpacitySlider");
            tintSlider.Value = 65;
            Check(Math.Abs(ThemeSettingsService.AppBackgroundPanelTintOpacity - .65) < .001,
                "The personalization slider persists percent-to-opacity conversion.");
            foreach (var theme in new[] { ElementTheme.Light, ElementTheme.Dark })
            {
                Named<RadioButton>(settings, theme == ElementTheme.Light ? "ThemeModeLightButton" : "ThemeModeDarkButton").IsChecked = true;
                await UntilAsync(() => page.TestFeatureTheme == theme, "Theme selection reaches the actual MainPage root.");
                if (!new AccessibilitySettings().HighContrast)
                {
                    await UntilAsync(() => (native.StyleGetFore(32) & 0xffffff) == (theme == ElementTheme.Light ? 0 : 0xffffff),
                        "Theme selection reaches the native editor foreground.");
                    Check((native.StyleGetFore(33) & 0xffffff) == (theme == ElementTheme.Light ? 0x696969 : 0x969696),
                        "Theme switching preserves the independently specified dim line-number colors.");
                    Check(unchecked((uint)native.GetElementColour(Element.SelectionBack)) == 0xffe22b8a &&
                        unchecked((uint)native.GetElementColour(Element.SelectionInactiveBack)) == 0xffe22b8a &&
                        unchecked((uint)native.GetElementColour(Element.SelectionText)) == 0xffffffff,
                        "Custom purple applies to active/inactive native selections with white selected text.");
                }
                Check(editor.TestNativeCore.GetSelectedText() == "selection" && editor.GetText() == "selection Ω😀\rsecond row",
                    "Theme and accent events preserve native selection and document text.");
                foreach (var orientation in new[] { Orientation.Horizontal, Orientation.Vertical })
                {
                    tintSlider.Orientation = orientation;
                    tintSlider.ApplyTemplate();
                    tintSlider.UpdateLayout();
                    var thumb = Descendants(tintSlider).OfType<Control>().Single(control => control.Name ==
                        (orientation == Orientation.Horizontal ? "HorizontalThumb" : "VerticalThumb"));
                    Check(VisualStateManager.GoToState(tintSlider, "PointerOver", false), "Custom slider provides PointerOver state.");
                    await UntilAsync(() => (thumb.Background as SolidColorBrush)?.Color == Color.FromArgb(255, 156, 75, 230),
                        "Slider hover uses the specified lighter purple in " + theme + "/" + orientation + ".");
                    Check(VisualStateManager.GoToState(tintSlider, "Normal", false), "Custom slider provides Normal state.");
                    await UntilAsync(() => (thumb.Background as SolidColorBrush)?.Color == Color.FromArgb(255, 138, 43, 226),
                        "Pointer exit restores the unmodified custom accent.");
                }
                tintSlider.Orientation = Orientation.Horizontal;
            }
            var activeThumb = Descendants(tintSlider).OfType<Control>().Single(control => control.Name == "HorizontalThumb");
            VisualStateManager.GoToState(tintSlider, "PointerOver", false);
            picker.Color = Color.FromArgb(255, 20, 100, 160);
            await UntilAsync(() => (activeThumb.Background as SolidColorBrush)?.Color == Color.FromArgb(255, 55, 123, 174),
                "Changing accent while hovered updates the existing thumb brush.");
            VisualStateManager.GoToState(tintSlider, "Pressed", false);
            await UntilAsync(() => (activeThumb.Background as SolidColorBrush)?.Color != Color.FromArgb(255, 55, 123, 174),
                "Pressed slider uses its own theme brush.");
            var wasEnabled = tintSlider.IsEnabled;
            tintSlider.IsEnabled = false;
            Check(VisualStateManager.GoToState(tintSlider, "Disabled", false), "Custom slider provides Disabled state.");
            await UntilAsync(() => (activeThumb.Background as SolidColorBrush)?.Color != Color.FromArgb(255, 55, 123, 174),
                "Disabled slider clears the accent hover brush.");
            tintSlider.IsEnabled = wasEnabled;
            VisualStateManager.GoToState(tintSlider, "Normal", false);
            Check(ThemeSettingsService.CustomAccentColor == Color.FromArgb(255, 20, 100, 160),
                "Custom color is retained independently from the Windows accent selection.");
            log.AppendLine("PASS: Appearance.ThemeAndAccent — actual personalization controls update MainPage, native selections and Light/Dark horizontal/vertical slider hover states.");
            CheckContrastPalettes(log);
        }
        finally
        {
            ThemeSettingsService.CustomAccentColor = originalCustomAccent;
            ThemeSettingsService.UseWindowsAccentColor = originalUseWindowsAccent;
            ThemeSettingsService.AppAccentColor = originalAccent;
            ThemeSettingsService.AppBackgroundPanelTintOpacity = originalTint;
            ThemeSettingsService.SetTheme(originalTheme);
            ThemeSettingsService.UseWindowsTheme = originalUseWindowsTheme;
        }
        var textSettings = await SettingsAsync<TextAndEditorSettingsPage>(page, "TextAndEditor");
        foreach (var (name, resourceKey) in new[]
        {
            ("LineHighlighterToggle", "TextAndEditorPage_LineHighlighterSettings_ToggleSwitch"),
            ("LineNumbersToggle", "TextAndEditorPage_LineNumbersSettings_ToggleSwitch"),
            ("WhitespaceToggle", "EditorWhitespace"),
            ("IndentGuidesToggle", "EditorIndentGuides"),
            ("BraceMatchingToggle", "EditorBraceMatching"),
            ("OccurrencesToggle", "EditorOccurrences")
        })
            CheckLocalizedToggleLayout(textSettings, name, resourceKey, log);
    }

    private static void CheckLocalizedToggleLayout(Page settings, string name, string resourceKey, StringBuilder log)
    {
        var toggle = Named<ToggleSwitch>(settings, name);
        var scroll = Descendants(settings).OfType<ScrollViewer>().First();
        var viewport = Descendants(scroll).OfType<FrameworkElement>()
            .First(element => element.Name == "ScrollContentPresenter");
        var verticalBar = Descendants(scroll).OfType<ScrollBar>()
            .First(bar => bar.Orientation == Orientation.Vertical);
        var resources = ResourceManager.Current.MainResourceMap.GetSubtree("Settings");
        var context = ResourceContext.GetForCurrentView().Clone();
        var originalOn = toggle.OnContent;
        var originalOff = toggle.OffContent;
        var originalSize = toggle.FontSize;
        var originalState = toggle.IsOn;
        var originalWidth = settings.Width;
        var originalDirection = settings.FlowDirection;
        try
        {
            foreach (var width in new[] { double.NaN, 280d })
            foreach (var language in new[] { "en-US", "zh-CN", "fr-FR", "pt-BR", "de-DE", "ja-JP", "ar-YE" })
            foreach (var fontSize in new[] { 14d, 28d })
            {
                settings.Width = width;
                settings.FlowDirection = language == "ar-YE" ? FlowDirection.RightToLeft : FlowDirection.LeftToRight;
                context.Languages = new[] { language };
                toggle.OnContent = resources.GetValue(resourceKey + "/OnContent", context).ValueAsString;
                toggle.OffContent = resources.GetValue(resourceKey + "/OffContent", context).ValueAsString;
                toggle.FontSize = fontSize;
                foreach (var isOn in new[] { false, true })
                {
                    toggle.IsOn = isOn;
                    settings.UpdateLayout();
                    var presenter = Descendants(toggle).OfType<ContentPresenter>().Single(element =>
                        element.Name == (isOn ? "OnContentPresenter" : "OffContentPresenter"));
                    var bounds = presenter.TransformToVisual(viewport).TransformBounds(
                        new Rect(0, 0, presenter.ActualWidth, presenter.ActualHeight));
                    var left = 0d;
                    var right = viewport.ActualWidth;
                    if (scroll.ComputedVerticalScrollBarVisibility == Visibility.Visible)
                    {
                        var barBounds = verticalBar.TransformToVisual(viewport).TransformBounds(
                            new Rect(0, 0, verticalBar.ActualWidth, verticalBar.ActualHeight));
                        // Overlay scrollbars can occupy part of the content viewport.
                        if (barBounds.Left + barBounds.Width / 2 >= viewport.ActualWidth / 2)
                            right = Math.Min(right, barBounds.Left);
                        else
                            left = Math.Max(left, barBounds.Right);
                    }
                    var description = $"{name}/{language}/{fontSize}/{width}/{isOn}";
                    Check(presenter.ActualWidth > 0 && bounds.Left >= left - 1 && bounds.Right <= right + 1,
                        "Translated toggle text stays inside the content viewport and clear of the scrollbar: " + description);
                    var probe = new TextBlock
                    {
                        Text = (string)presenter.Content,
                        FontFamily = presenter.FontFamily,
                        FontSize = presenter.FontSize,
                        FontWeight = presenter.FontWeight,
                        TextWrapping = TextWrapping.Wrap,
                        FlowDirection = settings.FlowDirection
                    };
                    probe.Measure(new Size(presenter.ActualWidth, double.PositiveInfinity));
                    Check(presenter.ActualHeight >= probe.DesiredSize.Height - 1,
                        "Translated toggle text has enough height for every wrapped line: " + description);
                    if (language == "fr-FR" && name == "BraceMatchingToggle")
                        Check(presenter.ActualHeight > fontSize * 1.5,
                            "The long French caption wraps onto multiple lines: " + description);
                }
            }
        }
        finally
        {
            toggle.OnContent = originalOn;
            toggle.OffContent = originalOff;
            toggle.FontSize = originalSize;
            toggle.IsOn = originalState;
            settings.Width = originalWidth;
            settings.FlowDirection = originalDirection;
            settings.UpdateLayout();
        }
        log.AppendLine($"PASS: Appearance.LocalizedToggleLayout/{name} — seven packaged translations fit the actual content viewport without overlapping the scrollbar in both switch states, at normal/narrow widths and 14/28-DIP text, including RTL.");
    }

    // Calls the production resource/palette branch with the current system colors;
    // it does not toggle the user's Windows contrast scheme or claim desktop coverage.
    private static void CheckContrastPalettes(StringBuilder log)
    {
        var system = new UISettings();
        var foreground = system.GetColorValue(UIColorType.Foreground);
        var background = system.GetColorValue(UIColorType.Background);
        var editor = EditorColorPalette.ForTheme(ElementTheme.Light, highContrast: true);
        Check(editor[EditorColorRole.Foreground] == foreground && editor[EditorColorRole.LineNumber] == foreground &&
            editor[EditorColorRole.Background] == background && editor[EditorColorRole.OpaqueBackground] == background &&
            editor[EditorColorRole.CaretLine].A == 0,
            "High contrast editor palette uses system text/background and suppresses caret shading.");
        var diff = DiffColorPalette.ForTheme(ElementTheme.Dark, highContrast: true);
        Check(diff[DiffColorRole.ActiveChangeBorder] == foreground && diff[DiffColorRole.GapHatch] == foreground &&
            diff[DiffColorRole.Gap] == background && diff[DiffColorRole.AddedText] == diff[DiffColorRole.DeletedText],
            "High contrast diff outline/hatch remain visible and do not rely on red/green distinction.");
        log.AppendLine("PASS: Appearance.ContrastPalette — production palette contracts use current system colors; OS contrast switching remains an external gate.");
    }

    internal static async Task RunZoomAndLanguageAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var page = host.Page;
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        editor.TypeText("def answer():\r    return 42");
        var initialText = editor.GetText();
        var initialNativeFont = editor.TestNativeCore.TestFeatureNative.StyleGetSizeFractional(32);
        page.TestFeatureOpenZoom();
        var slider = Named<Slider>(page, "FontZoomSlider");
        var zoomIn = Named<AppBarButton>(page, "ZoomIn");
        var zoomOut = Named<AppBarButton>(page, "ZoomOut");
        var reset = Named<AppBarButton>(page, "RestoreDefaultZoom");
        await UntilAsync(() => slider.IsLoaded && zoomIn.ActualHeight > 0 && reset.ActualHeight > 0,
            "The real status zoom flyout has completed layout.");
        try
        {
            foreach (var button in new[] { zoomIn, zoomOut })
            {
                Check(Math.Abs(button.ActualWidth - 32) < 1 && Math.Abs(button.ActualHeight - 32) < 1,
                    "Zoom +/- preserve their compact 32-DIP hit areas.");
                var icon = Descendants(button).OfType<FontIcon>().First();
                var bounds = icon.TransformToVisual(button).TransformBounds(new Rect(0, 0, icon.ActualWidth, icon.ActualHeight));
                Check(bounds.Top >= -1 && bounds.Bottom <= button.ActualHeight + 1 &&
                    Math.Abs(bounds.Top + bounds.Height / 2 - button.ActualHeight / 2) <= 2,
                    "Zoom glyph is centered and remains within the actual button template.");
            }
            var track = Descendants(slider).OfType<FrameworkElement>().Single(element => element.Name == "HorizontalTrackRect");
            var trackBounds = track.TransformToVisual((UIElement)page.TestFeatureZoomFlyout.Content)
                .TransformBounds(new Rect(0, 0, track.ActualWidth, track.ActualHeight));
            var buttonBounds = zoomIn.TransformToVisual((UIElement)page.TestFeatureZoomFlyout.Content)
                .TransformBounds(new Rect(0, 0, zoomIn.ActualWidth, zoomIn.ActualHeight));
            Check(Math.Abs(trackBounds.Top + trackBounds.Height / 2 - buttonBounds.Top - buttonBounds.Height / 2) <= 2,
                "The real slider track aligns with the zoom command centers.");
            Check(reset.Icon is FontIcon resetIcon && resetIcon.Glyph == "\uE72C" &&
                reset.ActualWidth >= 220 && !string.IsNullOrWhiteSpace(reset.Label),
                "Zoom reset retains the circular glyph and its localized label in the wide command row.");

            slider.Value = 113;
            Invoke(zoomIn);
            await UntilAsync(() => editor.GetFontZoomFactor() == 120 && Named<TextBlock>(page, "FontZoomIndicator").Text == "120%",
                "Zoom In snaps a fractional decade upward and updates status.");
            slider.Value = 113;
            Invoke(zoomOut);
            await UntilAsync(() => editor.GetFontZoomFactor() == 110, "Zoom Out snaps a fractional decade downward.");
            slider.Value = 500;
            Invoke(zoomIn);
            await UntilAsync(() => editor.GetFontZoomFactor() == 500, "Zoom In respects the maximum.");
            Check(editor.TestNativeCore.TestFeatureNative.StyleGetSizeFractional(32) == initialNativeFont * 5,
                "Maximum zoom changes native font geometry to five times the default.");
            slider.Value = 10;
            Invoke(zoomOut);
            await UntilAsync(() => editor.GetFontZoomFactor() == 10, "Zoom Out respects the minimum.");
            Invoke(reset);
            await UntilAsync(() => editor.GetFontZoomFactor() == 100 &&
                editor.TestNativeCore.TestFeatureNative.StyleGetSizeFractional(32) == initialNativeFont &&
                Named<TextBlock>(page, "FontZoomIndicator").Text == "100%", "Reset restores native size and visible status to 100 percent.");
            Check(editor.GetText() == initialText, "Zoom commands preserve document content.");
        }
        finally { page.TestFeatureZoomFlyout.Hide(); }
        log.AppendLine("PASS: Appearance.Zoom — actual flyout template geometry, decade rounding, both limits and reset/native-font/status agreement.");

        page.TestFeatureOpenLanguageMenu();
        var python = page.TestFeatureLanguageMenu.Items.OfType<MenuFlyoutItem>().Single(item => (string)item.Tag == "python");
        Invoke(python);
        await UntilAsync(() => editor.DocumentLanguage.Id == "python" && Named<TextBlock>(page, "LanguageIndicator").Text == "Python",
            "Language flyout command changes native language and status text.");
        Check(editor.LanguageOverride == "python" && editor.GetText() == initialText, "Manual language selection preserves document content.");
        page.TestFeatureOpenLanguageMenu();
        var stale = page.TestFeatureLanguageMenu.Items.OfType<MenuFlyoutItem>().Single(item => (string)item.Tag == "json");
        var added = await NewTabAsync(page);
        page.TestFeatureSelectLanguage(stale);
        Check(Editor(added).LanguageOverride == null && editor.LanguageOverride == "python",
            "A delayed language click cannot modify the newly selected editor.");
        host.Core.SwitchTo(editor);
        page.TestFeatureOpenLanguageMenu();
        var automatic = page.TestFeatureLanguageMenu.Items.OfType<MenuFlyoutItem>().Single(item => item.Tag == null);
        Invoke(automatic);
        await UntilAsync(() => editor.LanguageOverride == null && editor.DocumentLanguage.Id == "plaintext",
            "Automatic language restores filename detection for an untitled document.");
        log.AppendLine("PASS: Appearance.Language — real flyout selection, automatic detection and stale-target protection update status correctly.");
    }

    internal static async Task RunViewModeAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var view = ApplicationView.GetForCurrentView();
        Check(!view.IsFullScreenMode, "The owned integration window starts outside fullscreen.");
        try
        {
            host.Page.TestFeatureFullScreen();
            if (view.IsFullScreenMode)
            {
                host.Page.TestRefreshMenu();
                var label = ((MenuFlyoutItem)host.Page.TestFeatureControl("MenuFullScreenButton")).Text;
                Check(label == Windows.ApplicationModel.Resources.ResourceLoader.GetForCurrentView().GetString("App_ExitFullScreenMode_Text"),
                    "Accepted fullscreen entry updates the actual menu to Exit.");
                host.Page.TestFeatureFullScreen();
                Check(!view.IsFullScreenMode, "The actual fullscreen command restores the owned window.");
                log.AppendLine("PASS: ViewMode.FullscreenAccepted — real OS transition updates the menu and returns to normal; rejection not exercised.");
            }
            else
            {
                Check(host.Core.GetSelectedReadyEditor() != null, "Rejected fullscreen entry leaves the document available.");
                log.AppendLine("PASS: ViewMode.FullscreenRejected — OS refusal preserves the document; accepted transition not exercised.");
            }
        }
        finally { if (view.IsFullScreenMode) view.ExitFullScreenMode(); }

        string rejected = null;
        void Record(string name)
        {
            if (name is "FailedToEnterCompactOverlayViewMode" or "FailedToEnterDefaultViewMode") rejected = name;
        }
        AnalyticsService.EventRecorded += Record;
        try
        {
            Check(view.ViewMode == ApplicationViewMode.Default, "Owned view starts in normal mode before the overlay contract.");
            host.Page.TestFeatureCompactOverlay();
            await UntilAsync(() => view.ViewMode == ApplicationViewMode.CompactOverlay || rejected != null,
                "The real overlay command must complete with an OS transition or its production failure event.");
            if (rejected != null)
            {
                Check(view.ViewMode == ApplicationViewMode.Default && host.Page.TestFeatureStatusVisible &&
                    host.Page.TestFeatureControl("MainMenuButton").Visibility == Visibility.Visible,
                    "Rejected overlay entry preserves normal chrome and the live document.");
                log.AppendLine("PASS: ViewMode.OverlayRejected — actual OS refusal preserves normal chrome; accepted transition not exercised.");
            }
            else
            {
                await UntilAsync(() => host.Page.TestFeatureControl("ExitCompactOverlayButton").Visibility == Visibility.Visible &&
                    host.Page.TestFeatureControl("ExitCompactOverlayButton").IsLoaded &&
                    host.Page.TestFeatureControl("ExitCompactOverlayButton").ActualWidth > 0 &&
                    host.Page.TestFeatureControl("MainMenuButton").Visibility == Visibility.Collapsed &&
                    !host.Page.TestFeatureStatusVisible,
                    "Accepted overlay entry shows Exit and hides normal menu/status chrome.");
                AccessibilityFeatureTests.CheckExitOverlay(Named<Button>(host.Page, "ExitCompactOverlayButton"));
                Invoke(host.Page.TestFeatureControl("ExitCompactOverlayButton"));
                await UntilAsync(() => view.ViewMode == ApplicationViewMode.Default || rejected != null,
                    "The real Exit Overlay button completes its normal-mode request.");
                Check(rejected == null, "The owned overlay window can return to normal mode.");
                await UntilAsync(() => host.Page.TestFeatureControl("MainMenuButton").Visibility == Visibility.Visible &&
                    host.Page.TestFeatureControl("ExitCompactOverlayButton").Visibility == Visibility.Collapsed &&
                    host.Page.TestFeatureStatusVisible,
                    "Exiting overlay restores MainPage chrome and the status preference.");
                log.AppendLine("PASS: ViewMode.OverlayAccepted — real OS entry/exit updates menu, exit button and status chrome; rejection not exercised.");
            }
        }
        finally
        {
            AnalyticsService.EventRecorded -= Record;
            if (view.ViewMode == ApplicationViewMode.CompactOverlay)
                Check(await view.TryEnterViewModeAsync(ApplicationViewMode.Default), "Cleanup restores the owned window's normal view mode.");
        }
    }
}
