// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Preferences;
using Notepads.Features.WebSearch;
using Notepads.Infrastructure.Settings;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.Settings;
using Windows.ApplicationModel.Resources;
using Windows.UI.Text;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class PreferenceFeatureTests
{
    private const string RestartMarker = "IntegrationPreferencesRestartMarker";
    private static readonly string[] Keys =
    [
        SettingsKey.EditorFontFamilyStr, SettingsKey.EditorFontSizeInt, SettingsKey.EditorFontStyleStr,
        SettingsKey.EditorFontWeightUshort, SettingsKey.EditorFontRenderingStr, SettingsKey.EditorDefaultTextWrappingStr,
        SettingsKey.EditorDefaultLineHighlighterViewStateBool, SettingsKey.EditorDefaultDisplayLineNumbersBool,
        SettingsKey.EditorDefaultLineEndingStr, SettingsKey.EditorDefaultEncodingCodePageInt,
        SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool, SettingsKey.EditorDefaultDecodingCodePageInt,
        SettingsKey.EditorDefaultTabIndentsInt, SettingsKey.EditorDefaultSearchEngineStr,
        SettingsKey.EditorCustomMadeSearchUrlStr, SettingsKey.EditorShowStatusBarBool,
        SettingsKey.EditorEnableSessionBackupAndRestoreBool, SettingsKey.EditorHighlightMisspelledWordsBool,
        SettingsKey.EditorEnableSmartCopyBool, SettingsKey.AlwaysOpenNewWindowBool, SettingsKey.ExitWhenLastTabClosed
    ];

    internal static async Task RunAsync(StringBuilder log)
    {
        await RunRenderingAsync(log);
        using var settingsSnapshot = new StoredPreferences(Keys);
        await using var host = await FeatureTestHost.CreateAsync();
        var original = (TextEditor)host.Core.GetSelectedReadyEditor();
        original.TypeText("alpha\rbravo Ω😀\rcharlie");
        var originalText = original.GetText();
        var originalEncoding = original.GetEncoding();
        var originalLineEnding = original.GetLineEnding();
        var settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        Check(!Named<ToggleSwitch>(settings, "HighlightMisspelledWordsToggle").IsEnabled,
            "Unavailable native spelling is visibly disabled in the real settings page.");
        Named<ToggleSwitch>(settings, "TextWrappingToggle").IsOn = true;
        Named<ToggleSwitch>(settings, "LineNumbersToggle").IsOn = false;
        Named<ToggleSwitch>(settings, "LineHighlighterToggle").IsOn = false;
        Named<ComboBox>(settings, "FontFamilyPicker").SelectedItem = "Courier New";
        Named<ComboBox>(settings, "FontSizePicker").SelectedItem = 20;
        Named<ComboBox>(settings, "FontStylePicker").SelectedItem = settings.AvailableFontStyles
            .Single(item => item.FontStyle == FontStyle.Italic);
        Named<ComboBox>(settings, "FontWeightPicker").SelectedItem = settings.AvailableFontWeights
            .Single(item => item.FontWeight.Weight == 700);
        Named<RadioButton>(settings, "LfRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "Utf8BomEncodingRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "TabFourSpacesRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "Utf8DecodingRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "AutoGuessDecodingRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "GoogleRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "BingRadioButton").IsChecked = true;
        CheckRadioGroups(settings);
        await UntilAsync(() => NativePreferencesMatch(original), "Loaded native editor receives font, wrapping and gutter settings.");
        Check(original.GetText() == originalText && original.GetEncoding().CodePage == originalEncoding.CodePage &&
            original.GetEncoding().GetPreamble().SequenceEqual(originalEncoding.GetPreamble()) &&
            original.GetLineEnding() == originalLineEnding,
            "Live presentation preferences preserve existing text, encoding and line-ending intent.");
        Check((string)ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTextWrappingStr) == "Wrap" &&
            (int)ApplicationSettingsStore.Read(SettingsKey.EditorFontSizeInt) == 20 &&
            (ushort)ApplicationSettingsStore.Read(SettingsKey.EditorFontWeightUshort) == 700 &&
            (string)ApplicationSettingsStore.Read(SettingsKey.EditorDefaultLineEndingStr) == "Lf" &&
            (bool)ApplicationSettingsStore.Read(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool),
            "Real control events persist compatible typed values in the isolated LocalSettings store.");
        await CloseSettingsAsync(host.Page);
        var added = await NewTabAsync(host.Page);
        var newEditor = Editor(added);
        await UntilAsync(() => NativePreferencesMatch(newEditor), "A subsequently created editor inherits the same native preferences.");
        Check(newEditor.GetLineEnding() == LineEnding.Lf && newEditor.GetEncoding().GetPreamble().SequenceEqual(new byte[] { 0xef, 0xbb, 0xbf }),
            "A new document receives the chosen LF and UTF-8 BOM defaults.");
        log.AppendLine("PASS: Preferences.LiveAndNew — real controls persist typed values and update existing/new native editors without changing document content.");

        settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        var reopenedFontSize = Named<ComboBox>(settings, "FontSizePicker").SelectedItem;
        var reopenedWrap = Named<ToggleSwitch>(settings, "TextWrappingToggle").IsOn;
        var reopenedLf = Named<RadioButton>(settings, "LfRadioButton").IsChecked;
        var reopenedMatches = Equals(reopenedFontSize, 20) && reopenedWrap && reopenedLf == true;
        if (!reopenedMatches)
        {
            log.AppendLine($"DIAGNOSTIC: reopened font={reopenedFontSize ?? "null"}, wrap={reopenedWrap}, LF={reopenedLf?.ToString() ?? "null"}.");
            log.AppendLine($"DIAGNOSTIC: preferences font={ApplicationPreferences.EditorFontSize}, wrap={ApplicationPreferences.EditorDefaultWordWrap}, EOL={ApplicationPreferences.EditorDefaultLineEnding}.");
            foreach (var group in DescribeRadioGroups(settings)) log.AppendLine("DIAGNOSTIC: " + group);
        }
        Check(reopenedMatches, "Reopening Settings initializes controls from the persisted preferences.");
        CheckRadioGroups(settings);
        var advanced = await SettingsAsync<AdvancedSettingsPage>(host.Page, "Advanced");
        Check(ApplicationPreferences.EditorDefaultWordWrap &&
            newEditor.TestNativeCore.TestFeatureNative.WrapMode == WinUIEditor.Wrap.Word,
            "Navigating to Advanced preserves the selected wrapping preference and native layout.");
        Named<ToggleSwitch>(advanced, "EnableSmartCopyToggleSwitch").IsOn = true;
        Check(ApplicationPreferences.IsSmartCopyEnabled &&
            (bool)ApplicationSettingsStore.Read(SettingsKey.EditorEnableSmartCopyBool),
            "Advanced Smart Copy routes through the actual preference event.");
        var statusToggle = Named<ToggleSwitch>(advanced, "ShowStatusBarToggleSwitch");
        Check(statusToggle.Focus(FocusState.Programmatic), "The visible status preference accepts focus before interaction.");
        await UntilAsync(() => ReferenceEquals(Windows.UI.Xaml.Input.FocusManager.GetFocusedElement(), statusToggle),
            "The settings control owns focus before changing status-bar layout.");
        statusToggle.IsOn = false;
        await UntilAsync(() => !host.Page.TestFeatureStatusVisible, "Advanced preference unloads the actual status bar.");
        statusToggle.IsOn = true;
        await UntilAsync(() => host.Page.TestFeatureStatusVisible, "Advanced preference recreates the actual status bar.");
        await CloseSettingsAsync(host.Page);
        newEditor.TypeText("one\rtwo");
        newEditor.TestNativeCore.SetTextSelectionPosition(5, 5);
        var expectedPosition = string.Format(ResourceLoader.GetForCurrentView().GetString("TextEditor_LineColumnIndicator_ShortText"), 2, 2);
        await UntilAsync(() => host.Page.TestStatusPosition == expectedPosition && host.Page.TestStatusModified,
            "Recreated status bar reflects native caret position and subsequent document edits.");
        await SettingsAsync<AboutPage>(host.Page, "About");
        settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        CheckRadioGroups(settings);
        var wrapping = Named<ToggleSwitch>(settings, "TextWrappingToggle");
        Check(wrapping.IsOn, "Returning to Text and Editor preserves the selected wrapping control state.");
        var wrapChanges = 0;
        void CountWrapChanges(object sender, bool value) => wrapChanges++;
        ApplicationPreferences.OnDefaultTextWrappingChanged += CountWrapChanges;
        try
        {
            wrapping.IsOn = false;
            await UntilAsync(() => newEditor.TestNativeCore.TestFeatureNative.WrapMode == WinUIEditor.Wrap.None &&
                original.TestNativeCore.TestFeatureNative.WrapMode == WinUIEditor.Wrap.None,
                "Navigation rebinds settings events and updates both background and selected native editors.");
            Check(wrapChanges == 1, "One visible toggle after navigation emits exactly one preference change.");
        }
        finally { ApplicationPreferences.OnDefaultTextWrappingChanged -= CountWrapChanges; }
        log.AppendLine("PASS: Preferences.NavigationAndStatus — page navigation rebinds events; status unload/reload restores live caret/modified updates.");
        for (var reopen = 0; reopen < 3; reopen++)
        {
            await CloseSettingsAsync(host.Page);
            settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
            CheckRadioGroups(settings);
            Check(!Named<ToggleSwitch>(settings, "TextWrappingToggle").IsOn &&
                newEditor.TestNativeCore.TestFeatureNative.WrapMode == WinUIEditor.Wrap.None &&
                original.TestNativeCore.TestFeatureNative.WrapMode == WinUIEditor.Wrap.None,
                "Repeated settings opens retain the chosen preference and both native editor layouts.");
        }
        log.AppendLine("PASS: Preferences.RadioGroups — repeated real pane open/close and page navigation retain five independent mutually exclusive radio groups.");
    }

    private static bool NativePreferencesMatch(TextEditor editor)
    {
        var native = editor.TestNativeCore.TestFeatureNative;
        return native.WrapMode == WinUIEditor.Wrap.Word && native.GetMarginWidthN(0) == 0 &&
            !native.CaretLineVisible && native.StyleGetFont(32) == "Courier New" &&
            native.StyleGetSizeFractional(32) == 1500 && native.StyleGetItalic(32) &&
            (int)native.StyleGetWeight(32) == 700;
    }

    private static void CheckRadioGroups(TextAndEditorSettingsPage settings)
    {
        foreach (var (panel, selected) in new[]
        {
            ("LineEndingPanel", "LfRadioButton"), ("EncodingPanel", "Utf8BomEncodingRadioButton"),
            ("DecodingPanel", "AutoGuessDecodingRadioButton"), ("TabBehaviorPanel", "TabFourSpacesRadioButton"),
            ("SearchEnginePanel", "BingRadioButton")
        })
        {
            var checkedNames = Named<StackPanel>(settings, panel).Children.OfType<RadioButton>()
                .Where(button => button.IsChecked == true).Select(button => button.Name).ToArray();
            Check(checkedNames.SequenceEqual(new[] { selected }),
                "Each settings radio group remains mutually exclusive and independent after navigation: " + panel +
                "; expected=" + selected + "; actual=" + string.Join(",", checkedNames));
        }
    }

    private static IEnumerable<string> DescribeRadioGroups(TextAndEditorSettingsPage settings) =>
        new[] { "LineEndingPanel", "EncodingPanel", "DecodingPanel", "TabBehaviorPanel", "SearchEnginePanel" }
            .Select(name => name + "=[" + string.Join(",", ((StackPanel)settings.FindName(name)).Children.OfType<RadioButton>()
                .Where(button => button.IsChecked == true).Select(button => button.Name)) + "]");

    internal static async Task RunStoreAsync(StringBuilder log)
    {
        using var snapshot = new StoredPreferences(Keys);
        foreach (var key in Keys) ApplicationSettingsStore.Remove(key);
        ApplicationPreferences.Initialize();
        Check(ApplicationPreferences.EditorFontFamily == "Consolas" && ApplicationPreferences.EditorFontSize == 14 &&
            ApplicationPreferences.EditorFontRendering == FontRenderingMode.Grayscale &&
            ApplicationPreferences.EditorFontWeight == 400 && ApplicationPreferences.EditorFontStyle == FontSlant.Normal &&
            !ApplicationPreferences.EditorDefaultWordWrap && ApplicationPreferences.EditorDisplayLineHighlighter &&
            ApplicationPreferences.EditorDisplayLineNumbers && ApplicationPreferences.ShowStatusBar,
            "An empty real settings container yields established presentation defaults.");
        Check(ApplicationPreferences.EditorDefaultEncoding.CodePage == 65001 &&
            ApplicationPreferences.EditorDefaultEncoding.GetPreamble().Length == 0 &&
            ApplicationPreferences.EditorDefaultDecoding == null && ApplicationPreferences.EditorDefaultLineEnding == LineEnding.Crlf &&
            ApplicationPreferences.EditorDefaultTabIndents == 4 && ApplicationPreferences.IsSessionSnapshotEnabled &&
            !ApplicationPreferences.IsSmartCopyEnabled && !ApplicationPreferences.ExitWhenLastTabClosed,
            "An empty settings container yields established document and lifecycle defaults.");
        await using (var host = await FeatureTestHost.CreateAsync())
        {
            var draft = (TextEditor)host.Core.GetSelectedReadyEditor();
            draft.TestNativeCore.TestIndentation(false);
            Check(draft.GetText() == "    " && !draft.TestNativeCore.TestFeatureNative.UseTabs,
                "A fresh plain-text draft inserts four spaces through the real Tab command with unset preferences.");
        }

        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTextWrappingStr, "WrapWholeWords");
        ApplicationSettingsStore.Write(SettingsKey.EditorFontRenderingStr, "ClearType");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultEncodingCodePageInt, 65001);
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool, true);
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineEndingStr, "Cr");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultSearchEngineStr, "DuckDuckGo");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTabIndentsInt, -1);
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, false);
        ApplicationPreferences.Initialize();
        Check(ApplicationPreferences.EditorDefaultWordWrap && ApplicationPreferences.EditorDefaultLineEnding == LineEnding.Cr &&
            ApplicationPreferences.EditorFontRendering == FontRenderingMode.ClearType &&
            ApplicationPreferences.EditorDefaultEncoding.GetPreamble().SequenceEqual(new byte[] { 0xef, 0xbb, 0xbf }) &&
            ApplicationPreferences.EditorDefaultSearchEngine == SearchEngine.DuckDuckGo &&
            ApplicationPreferences.EditorDefaultTabIndents == -1 && !ApplicationPreferences.IsSessionSnapshotEnabled,
            "Legacy wrapping and search-key values retain their original persisted semantics.");

        ApplicationSettingsStore.Write(SettingsKey.EditorFontSizeInt, "twenty");
        ApplicationSettingsStore.Write(SettingsKey.EditorFontWeightUshort, "bold");
        ApplicationSettingsStore.Write(SettingsKey.EditorFontStyleStr, "not-a-style");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTextWrappingStr, "not-a-wrap-mode");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineHighlighterViewStateBool, "false");
        ApplicationSettingsStore.Write(SettingsKey.EditorFontRenderingStr, "unknown-rendering-mode");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultLineEndingStr, "not-a-line-ending");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultDecodingCodePageInt, -12345);
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultSearchEngineStr, "not-a-search-engine");
        ApplicationSettingsStore.Write(SettingsKey.EditorDefaultTabIndentsInt, "four");
        ApplicationSettingsStore.Write(SettingsKey.EditorEnableSessionBackupAndRestoreBool, "false");
        ApplicationPreferences.Initialize();
        Check(ApplicationPreferences.EditorFontSize == 14 && ApplicationPreferences.EditorFontWeight == 400 &&
            ApplicationPreferences.EditorFontRendering == FontRenderingMode.Grayscale &&
            ApplicationPreferences.EditorFontStyle == FontSlant.Normal && !ApplicationPreferences.EditorDefaultWordWrap &&
            ApplicationPreferences.EditorDisplayLineHighlighter && ApplicationPreferences.EditorDefaultLineEnding == LineEnding.Crlf &&
            ApplicationPreferences.EditorDefaultDecoding == null && ApplicationPreferences.EditorDefaultSearchEngine == SearchEngine.Bing &&
            ApplicationPreferences.EditorDefaultTabIndents == 4 && ApplicationPreferences.IsSessionSnapshotEnabled,
            "Wrong-type and malformed settings fall back predictably; invalid decoding selects automatic detection.");
        log.AppendLine("PASS: Preferences.Store — empty defaults, valid/legacy values and corrupt typed settings use independent expected values.");
    }

    internal static Task RunCorruptEncodingAsync(StringBuilder log)
    {
        using var snapshot = new StoredPreferences(Keys);
        foreach (var codePage in new[] { -12345, 65536, 65535 })
        {
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultEncodingCodePageInt, codePage);
            ApplicationSettingsStore.Write(SettingsKey.EditorDefaultUtf8EncoderShouldEmitByteOrderMarkBool, true);
            ApplicationPreferences.Initialize();
            Check(ApplicationPreferences.EditorDefaultEncoding.CodePage == 65001 &&
                ApplicationPreferences.EditorDefaultEncoding.GetPreamble().Length == 0,
                "An invalid persisted default encoding must fall back to UTF-8 without BOM: " + codePage);
        }
        log.AppendLine("PASS: Preferences.CorruptEncoding — malformed persisted code page is contained during startup.");
        return Task.CompletedTask;
    }

    internal static async Task SeedAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var personalization = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        Named<RadioButton>(personalization, "ClearTypeRenderingRadioButton").IsChecked = true;
        var settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        Named<ToggleSwitch>(settings, "TextWrappingToggle").IsOn = true;
        Named<ToggleSwitch>(settings, "LineNumbersToggle").IsOn = false;
        Named<ComboBox>(settings, "FontSizePicker").SelectedItem = 20;
        Named<RadioButton>(settings, "LfRadioButton").IsChecked = true;
        Named<RadioButton>(settings, "Utf8BomEncodingRadioButton").IsChecked = true;
        var advanced = await SettingsAsync<AdvancedSettingsPage>(host.Page, "Advanced");
        Named<ToggleSwitch>(advanced, "EnableSmartCopyToggleSwitch").IsOn = true;
        ApplicationSettingsStore.Write(RestartMarker, "controls-seeded-v1");
        Check((string)ApplicationSettingsStore.Read(SettingsKey.EditorDefaultTextWrappingStr) == "Wrap" &&
            (int)ApplicationSettingsStore.Read(SettingsKey.EditorFontSizeInt) == 20,
            "The seed phase persists settings through real UI events.");
        log.AppendLine("PASS: Preferences.RestartSeed — actual controls wrote retained private settings for a fresh process.");
    }

    internal static async Task VerifyRestartAsync(StringBuilder log)
    {
        Check((string)ApplicationSettingsStore.Read(RestartMarker) == "controls-seeded-v1",
            "The restart phase received the explicit seed container.");
        Check(ApplicationPreferences.EditorDefaultWordWrap && ApplicationPreferences.EditorFontSize == 20 &&
            ApplicationPreferences.EditorFontRendering == FontRenderingMode.ClearType &&
            !ApplicationPreferences.EditorDisplayLineNumbers && ApplicationPreferences.EditorDefaultLineEnding == LineEnding.Lf &&
            ApplicationPreferences.IsSmartCopyEnabled &&
            ApplicationPreferences.EditorDefaultEncoding.GetPreamble().SequenceEqual(new byte[] { 0xef, 0xbb, 0xbf }),
            "Fresh-process preference initialization reads the UI-seeded settings.");
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        var native = editor.TestNativeCore.TestFeatureNative;
        Check(native.WrapMode == WinUIEditor.Wrap.Word && native.GetMarginWidthN(0) == 0 &&
            native.FontQuality == WinUIEditor.FontQuality.QualityLcdOptimized &&
            native.StyleGetSizeFractional(32) == 1500 && editor.GetLineEnding() == LineEnding.Lf &&
            editor.GetEncoding().GetPreamble().SequenceEqual(new byte[] { 0xef, 0xbb, 0xbf }),
            "Fresh native editor receives persisted presentation and document defaults after process restart.");
        var settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
        Check(Named<ToggleSwitch>(settings, "TextWrappingToggle").IsOn &&
            !Named<ToggleSwitch>(settings, "LineNumbersToggle").IsOn &&
            (int)Named<ComboBox>(settings, "FontSizePicker").SelectedItem == 20,
            "Fresh settings controls agree with the persisted state applied to the editor.");
        var personalization = await SettingsAsync<PersonalizationSettingsPage>(host.Page, "Personalization");
        Check(Named<RadioButton>(personalization, "ClearTypeRenderingRadioButton").IsChecked == true &&
            !Named<Slider>(personalization, "BackgroundTintOpacitySlider").IsEnabled &&
            Named<TextBlock>(personalization, "ClearTypeTransparencyNotice").Visibility == Visibility.Visible,
            "Fresh personalization restores ClearType and its transparency constraint together.");
        ApplicationSettingsStore.Remove(RestartMarker);
        log.AppendLine("PASS: Preferences.Restart — a fresh process, settings controls and native editor agree on retained values.");
    }

    private sealed partial class StoredPreferences : IDisposable
    {
        private readonly Dictionary<string, object> _saved;
        internal StoredPreferences(IEnumerable<string> keys) => _saved = keys.ToDictionary(key => key, ApplicationSettingsStore.Read);
        public void Dispose()
        {
            foreach (var entry in _saved)
                if (entry.Value == null) ApplicationSettingsStore.Remove(entry.Key);
                else ApplicationSettingsStore.Write(entry.Key, entry.Value);
            // Callers dispose their host first. Initialize only rereads fields, without adding subscriptions.
            ApplicationPreferences.Initialize();
        }
    }
}
