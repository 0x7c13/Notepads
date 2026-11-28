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
        var editors = new[] { core, hidden.TestNativeCore, oldPane, newPane };
        Check(editors.All(editor => editor.TestFeatureNative.FontQuality == FontQuality.QualityAntialiased),
            "Visible, hidden and Diff editors start in grayscale.");
        var settings = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        CheckRenderingSettings(settings);
        var theme = ThemeSettingsService.ThemeMode;
        var useWindowsTheme = ThemeSettingsService.UseWindowsTheme;
        var oldSelection = (native.Anchor, native.CurrentPos);
        var oldCanUndo = native.CanUndo();
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
                    await EditorTestLayout.AfterPaintAsync(native, () =>
                    {
                        foreach (var editor in editors) editor.SetFontRendering(mode);
                    });
                    var quality = mode == FontRenderingMode.ClearType ? FontQuality.QualityLcdOptimized : FontQuality.QualityAntialiased;
                    Check(editors.All(editor => editor.TestFeatureNative.FontQuality == quality),
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
            CheckRenderingSettings(settings);
            native.Undo();
            Check(selected.GetText() == "", "Rendering switches preserve the original edit's undo operation.");
        }
        finally
        {
            ThemeSettingsService.SetTheme(theme);
            ThemeSettingsService.UseWindowsTheme = useWindowsTheme;
        }
        log.AppendLine("PASS: grayscale defaults, Personalization controls and internal font rendering across themes, hidden editors and Diff.");
    }

    private static void CheckRenderingSettings(PersonalizationSettingsPage settings)
    {
        Check(settings.FindName("FontRenderingPanel") == null &&
            settings.FindName("FontRenderingDescription") == null &&
            settings.FindName("ClearTypeTransparencyNotice") == null,
            "Personalization omits font rendering controls and explanatory text.");
        var slider = Named<Slider>(settings, "BackgroundTintOpacitySlider");
        var system = new UISettings();
        Check(slider.IsEnabled == (system.AdvancedEffectsEnabled && PowerManager.EnergySaverStatus != EnergySaverStatus.On),
            "Transparency controls respect the Windows effects and energy-saver policy.");
        Check(slider.Value == ThemeSettingsService.AppBackgroundPanelTintOpacity * 100,
            "Personalization shows the retained transparency value.");
    }
}
