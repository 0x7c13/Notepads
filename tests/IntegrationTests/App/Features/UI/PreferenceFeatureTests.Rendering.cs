// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.Settings;
using NotepadsEditorTests;
using Windows.Graphics.Display;
using Windows.System.Power;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class PreferenceFeatureTests
{
    private static async Task RunRenderingAsync(StringBuilder log)
    {
        using var saved = new StoredPreferences(new[] { SettingsKey.EditorFontRenderingStr });
        ApplicationPreferences.EditorFontRendering = FontRenderingMode.Grayscale;
        await using var host = await FeatureTestHost.CreateAsync();
        var hidden = (TextEditor)host.Core.GetSelectedReadyEditor();
        hidden.TypeText("hidden Ω😀");
        var selected = Editor(await NewTabAsync(host.Page));
        var core = selected.TestNativeCore;
        // A workspace may be wider than this sample on a large monitor. Bound
        // the real editor so the pan below is legal rather than raw overscroll.
        core.Width = 600;
        core.HorizontalAlignment = HorizontalAlignment.Left;
        core.TextWrapping = TextWrapping.NoWrap;
        core.RightToLeftReadingOrder = true;
        var source = string.Join('\r', Enumerable.Repeat("ABC אבג العربية 中文 " + new string('x', 160), 100));
        selected.TypeText(source);
        core.SetTextSelectionPosition(2, 10);
        var native = core.TestFeatureNative;
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        using var baseline = await DocumentBaseline.FromTextAsync("old Ω😀", Guid.NewGuid());
        using var diff = new SideBySideDiffViewer();
        Check(await diff.PrepareAsync(baseline, core, DocumentLanguages.PlainText, "rendering.txt", CancellationToken.None),
            "The existing Diff snapshot is prepared before changing font rendering.");
        var oldPane = Named<TextEditorCore>(diff, "OldEditor");
        var newPane = Named<TextEditorCore>(diff, "NewEditor");
        var settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        var gray = Named<RadioButton>(settings, "GrayscaleRenderingRadioButton");
        var lcd = Named<RadioButton>(settings, "ClearTypeRenderingRadioButton");
        Check(gray.IsChecked == true && lcd.IsChecked == false &&
            !string.IsNullOrWhiteSpace(Named<TextBlock>(settings, "FontRenderingDescription").Text),
            "Real settings expose the grayscale default and its localized explanation.");
        var description = Named<TextBlock>(settings, "FontRenderingDescription");
        var tintTitle = Named<TextBlock>(settings, "BackgroundTintOpacityTitle");
        Check(description.ActualHeight > 0 && description.TransformToVisual(settings)
            .TransformPoint(new(0, description.ActualHeight)).Y <= tintTitle.TransformToVisual(settings).TransformPoint(default).Y,
            "Personalization presents font rendering and its explanation before background tint opacity.");
        var theme = ThemeSettingsService.ThemeMode;
        var useWindowsTheme = ThemeSettingsService.UseWindowsTheme;
        var oldSelection = (native.Anchor, native.CurrentPos);
        var oldCanUndo = native.CanUndo();
        var changes = 0;
        void Changed(object sender, FontRenderingMode mode) => changes++;
        ApplicationPreferences.OnFontRenderingChanged += Changed;
        try
        {
            foreach (var nextTheme in new[] { ElementTheme.Light, ElementTheme.Dark })
            {
                var themeButton = Named<RadioButton>(settings,
                    nextTheme == ElementTheme.Light ? "ThemeModeLightButton" : "ThemeModeDarkButton");
                if (ThemeSettingsService.ThemeMode != nextTheme)
                    await EditorTestLayout.AfterPaintAsync(native, () => themeButton.IsChecked = true);
                else themeButton.IsChecked = true;
                await UntilAsync(() => host.Page.TestFeatureTheme == nextTheme, "The production page applies the test theme.");
                var offset = native.TextHeight(0) / DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel * .375;
                if (native.FirstVisibleLine != 17 || view.DisplayVerticalOffset != offset)
                    await EditorTestLayout.AfterPaintAsync(native, () => view.SetVerticalViewport(17, offset));
                Check(native.TextWidth(32, new string('x', 160)) >
                    view.ActualWidth * DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel + 72,
                    "Measured text exceeds the real viewport and permits the requested horizontal pan.");
                if (native.XOffset != 72)
                    await EditorTestLayout.AfterPaintAsync(native, () => native.XOffset = 72);
                var leadingOffset = view.DisplayHorizontalOffset;
                var glyphX = native.PointXFromPosition(native.PositionFromLine(17));
                Check(native.FirstVisibleLine == 17 && view.DisplayVerticalOffset == offset && native.XOffset == 72,
                    $"The fixture starts partially scrolled: row={native.FirstVisibleLine}, offset={view.DisplayVerticalOffset}/{offset}, x={native.XOffset}.");
                foreach (var mode in new[] { FontRenderingMode.ClearType, FontRenderingMode.Grayscale })
                {
                    var before = changes;
                    await EditorTestLayout.AfterPaintAsync(native, () =>
                        (mode == FontRenderingMode.ClearType ? lcd : gray).IsChecked = true);
                    Check(changes == before + 1 && (string)ApplicationSettingsStore.Read(SettingsKey.EditorFontRenderingStr) == mode.ToString(),
                        "One radio choice persists and broadcasts one rendering change.");
                    Check(themeButton.IsChecked == true && gray.IsChecked != lcd.IsChecked &&
                        (mode == FontRenderingMode.ClearType ? lcd : gray).IsChecked == true,
                        "Theme and font-rendering radio groups remain independent on the same page.");
                    ApplicationPreferences.EditorFontRendering = mode;
                    Check(changes == before + 1, "Reapplying the same mode does not restyle documents.");
                    var quality = mode == FontRenderingMode.ClearType ? FontQuality.QualityLcdOptimized : FontQuality.QualityAntialiased;
                    // Each editor owns a dispatcher callback. A paint from the
                    // selected editor does not drain the other editors' queues.
                    await UntilAsync(() => native.FontQuality == quality &&
                        hidden.TestNativeCore.TestFeatureNative.FontQuality == quality &&
                        oldPane.TestFeatureNative.FontQuality == quality && newPane.TestFeatureNative.FontQuality == quality,
                        "Visible, hidden and existing Diff editors adopt the new rendering mode.");
                    var highContrast = new AccessibilitySettings().HighContrast;
                    if (mode == FontRenderingMode.ClearType || highContrast)
                    {
                        var expected = highContrast
                            ? new UISettings().GetColorValue(UIColorType.Background)
                            : nextTheme == ElementTheme.Light ? Color.FromArgb(255, 240, 240, 240) : Color.FromArgb(255, 37, 37, 37);
                        Check(((Grid)core.Content).Background is SolidColorBrush brush && brush.Color == expected &&
                            native.StyleGetBack(32) == (expected.R | expected.G << 8 | expected.B << 16) &&
                            native.StyleGetBack(33) == native.StyleGetBack(32),
                            "ClearType and high contrast use the expected solid theme/system color for native text, gutter and XAML chrome.");
                    }
                    else Check(((Grid)core.Content).Background == null, "Grayscale releases the forced solid backdrop, including its insets.");
                    Check((native.Anchor, native.CurrentPos) == oldSelection && native.CanUndo() == oldCanUndo &&
                        selected.GetText() == source && hidden.GetText() == "hidden Ω😀",
                        "Rendering changes preserve text, selection and undo.");
                    Check(native.FirstVisibleLine == 17 && view.DisplayVerticalOffset == offset && native.XOffset == 72 &&
                        view.DisplayHorizontalOffset == leadingOffset && native.PointXFromPosition(native.PositionFromLine(17)) == glyphX,
                        $"Rendering preserves the RTL viewport: row={native.FirstVisibleLine}, offset={view.DisplayVerticalOffset}/{offset}, x={native.XOffset}; mode={mode}.");
                    Check(oldPane.GetText() == "old Ω😀" && newPane.GetText() == source &&
                        oldPane.TestFeatureNative.ReadOnly && newPane.TestFeatureNative.ReadOnly && diff.HasChanges,
                        "Font rendering retains read-only Diff documents and hunk presentation.");
                }
            }
            // Simulate another process publishing the shared preference while
            // this actual Settings page stays open; activation uses this reader.
            ApplicationSettingsStore.Write(SettingsKey.EditorFontRenderingStr, "ClearType");
            ApplicationPreferences.RefreshFontRenderingSetting();
            await UntilAsync(() => lcd.IsChecked == true && native.FontQuality == FontQuality.QualityLcdOptimized,
                "An external setting update refreshes both editor and already-open radio controls.");
            var count = changes;
            ApplicationPreferences.RefreshFontRenderingSetting();
            Check(changes == count, "Unchanged activation refresh emits no restyling event.");
            ApplicationSettingsStore.Write(SettingsKey.EditorFontRenderingStr, 42);
            ApplicationPreferences.RefreshFontRenderingSetting();
            await UntilAsync(() => gray.IsChecked == true && native.FontQuality == FontQuality.QualityAntialiased,
                "A malformed external value falls back to grayscale in the editor and settings UI.");
            Check(ApplicationSettingsStore.Read(SettingsKey.EditorFontRenderingStr) is int value && value == 42,
                "Refreshing the radio selection never writes back to the shared store.");
            await CloseSettingsAsync(host.Page);
            settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
            Check(Named<RadioButton>(settings, "GrayscaleRenderingRadioButton").IsChecked == true,
                "Reopened settings retain the current rendering mode after event rebinding.");
            await CheckTransparencyControlsAsync(host);
            native.Undo();
            Check(selected.GetText() == "", "Rendering switches preserve the original edit's undo operation.");
        }
        finally
        {
            ApplicationPreferences.OnFontRenderingChanged -= Changed;
            ThemeSettingsService.SetTheme(theme);
            ThemeSettingsService.UseWindowsTheme = useWindowsTheme;
        }
        log.AppendLine("PASS: font rendering — real settings, Light/Dark, hidden editors, Diff, selection/viewport/undo and external-setting refresh.");
    }

    private static async Task CheckTransparencyControlsAsync(FeatureTestHost host)
    {
        var settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        var slider = Named<Slider>(settings, "BackgroundTintOpacitySlider");
        var notice = Named<TextBlock>(settings, "ClearTypeTransparencyNotice");
        var value = slider.Value;
        var tint = ThemeSettingsService.AppBackgroundPanelTintOpacity;
        var system = new UISettings();
        Check(notice.Visibility == Visibility.Collapsed && slider.IsEnabled ==
            (system.AdvancedEffectsEnabled && PowerManager.EnergySaverStatus != EnergySaverStatus.On),
            "Grayscale transparency controls respect the existing Windows effects and energy-saver policy.");
        Named<RadioButton>(settings, "ClearTypeRenderingRadioButton").IsChecked = true;
        await UntilAsync(() => !slider.IsEnabled && notice.Visibility == Visibility.Visible,
            "Choosing ClearType on the same page disables transparency and displays the localized reason.");
        Check(!string.IsNullOrWhiteSpace(notice.Text) && notice.Text.Contains("ClearType", StringComparison.Ordinal),
            "The visible explanation identifies the rendering mode that requires a solid background.");
        foreach (var theme in new[] { ElementTheme.Light, ElementTheme.Dark })
        {
            ThemeSettingsService.SetTheme(theme);
            var expected = new AccessibilitySettings().HighContrast
                ? (Color)Application.Current.Resources["SystemColorWindowTextColor"]
                : theme == ElementTheme.Light ? Color.FromArgb(255, 138, 91, 0) : Color.FromArgb(255, 255, 217, 102);
            await UntilAsync(() => (notice.Foreground as SolidColorBrush)?.Color == expected,
                "The explanation uses readable amber/yellow theme colors (system text in high contrast).");
        }
        await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        slider = Named<Slider>(settings, "BackgroundTintOpacitySlider");
        notice = Named<TextBlock>(settings, "ClearTypeTransparencyNotice");
        Check(!slider.IsEnabled && notice.Visibility == Visibility.Visible,
            "Reopening personalization initializes the ClearType constraint before interaction.");
        ApplicationSettingsStore.Write(SettingsKey.EditorFontRenderingStr, "Grayscale");
        ApplicationPreferences.RefreshFontRenderingSetting();
        await UntilAsync(() => notice.Visibility == Visibility.Collapsed && slider.IsEnabled ==
            (system.AdvancedEffectsEnabled && PowerManager.EnergySaverStatus != EnergySaverStatus.On) &&
            Named<RadioButton>(settings, "GrayscaleRenderingRadioButton").IsChecked == true,
            "External refresh to grayscale restores the Windows-dependent availability and removes the notice.");
        Check(slider.Value == value && ThemeSettingsService.AppBackgroundPanelTintOpacity == tint,
            "Rendering switches and page navigation preserve the user's original transparency value.");
    }
}
