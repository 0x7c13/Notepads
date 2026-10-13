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
using Windows.Foundation;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
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
            editor[EditorColorRole.Background] == background && editor[EditorColorRole.CaretLine].A == 0,
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
                    host.Page.TestFeatureControl("MainMenuButton").Visibility == Visibility.Collapsed &&
                    !host.Page.TestFeatureStatusVisible,
                    "Accepted overlay entry shows Exit and hides normal menu/status chrome.");
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
