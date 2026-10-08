// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Buffers.Binary;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.ApplicationModel.Activation;
using Windows.Graphics.Display;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Media;

namespace NotepadsEditorTests;

public static class Program
{
    public static void Main(string[] args)
    {
        try
        {
            Application.Start(_ =>
            {
                System.Threading.SynchronizationContext.SetSynchronizationContext(new Windows.System.DispatcherQueueSynchronizationContext(
                    Windows.System.DispatcherQueue.GetForCurrentThread()));
                new App();
            });
        }
        catch (Exception ex)
        {
            System.IO.File.WriteAllText(System.IO.Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), "FAIL: startup " + ex);
        }
    }
}

public sealed partial class App : Application
{
    private const int DefaultStyle = (int)WinUIEditor.StylesCommon.Default;
    private const int LineNumberStyle = (int)WinUIEditor.StylesCommon.LineNumber;
    private const int IllegalMethodCall = unchecked((int)0x8000000E); // E_ILLEGAL_METHOD_CALL
    private const int ObjectClosed = unchecked((int)0x80000013); // RO_E_CLOSED

    public App()
    {
        UnhandledException += (_, e) => WriteDiagnostic("Unhandled: " + e.Message + "\n" + e.Exception);
        WriteDiagnostic("Constructing test application");
        InitializeComponent();
        WriteDiagnostic("Test application constructed");
    }

    private static void WriteDiagnostic(string text) => System.IO.File.WriteAllText(
        System.IO.Path.Combine(ApplicationData.Current.LocalFolder.Path, "diagnostic.txt"), text);

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        WriteDiagnostic("Launching tests");
        var log = new StringBuilder();
        try
        {
            if (args.Arguments.StartsWith("--lease-", StringComparison.Ordinal))
            {
                await SessionLeaseProcessTests.RunAsync(args.Arguments);
                Exit();
                return;
            }
            if (args.Arguments == "--performance")
            {
                ThemeSettingsService.Initialize();
                ApplicationPreferences.Initialize();
                await SessionPerformanceTests.RunAsync(log);
                CompleteTests(log);
                return;
            }
            if (args.Arguments is "--persistence" or "--session-resilience")
            {
                ThemeSettingsService.Initialize();
                ApplicationPreferences.Initialize();
                if (args.Arguments == "--persistence") await DocumentBaselineTests.RunAsync(log);
                await SessionStorageTests.RunAsync(log);
                await SessionRecoveryTests.RunAsync(log);
                await SessionServiceTests.RunAsync(log);
                if (args.Arguments == "--persistence")
                {
                    await SessionRegistryTests.RunAsync(log);
                    await SessionControllerTests.RunAsync(log);
                }
                CompleteTests(log);
                return;
            }
            await CheckColdNativeFactoryAsync();
            log.AppendLine("PASS: journal static factory activates from packaged metadata before any native editor exists.");
            ThemeSettingsService.Initialize();
            ApplicationPreferences.Initialize();
            WriteDiagnostic("Constructing native control");
            var core = new TextEditorCore();
            WriteDiagnostic("Editor adapter constructed");
            var lineNumberReveal = (Border)((Grid)core.Content).Children[1];
            Check(lineNumberReveal.BorderThickness.Right == 0, "line-number hover border hidden by default");
            var backdrop = new Grid
            {
                Background = new Windows.UI.Xaml.Media.LinearGradientBrush
                {
                    StartPoint = new Windows.Foundation.Point(0, 0),
                    EndPoint = new Windows.Foundation.Point(1, 1),
                    GradientStops =
                {
                    new Windows.UI.Xaml.Media.GradientStop { Color = Windows.UI.Color.FromArgb(255, 30, 36, 52), Offset = 0 },
                    new Windows.UI.Xaml.Media.GradientStop { Color = Windows.UI.Color.FromArgb(255, 64, 48, 36), Offset = 1 }
                }
                }
            };
            backdrop.Children.Add(core);
            Window.Current.Content = backdrop;
            WriteDiagnostic("Editor attached to window");
            Window.Current.Activate();
            WriteDiagnostic("Window activated; importing initial text");
            const string imported = "你好😀\r\nx\nz\r\0end\r";
            const string normalized = "你好😀\rx\rz\r\0end\r";
            core.SetText(imported);
            WriteDiagnostic("Initial text loaded");
            var initialText = core.GetText();
            var nativeView = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
            var native = nativeView.Editor;
            Check((native.StyleGetFore(LineNumberStyle) & 0xffffff) == 0x969696 &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.CaretLineBack)) == 0x18ffffff,
                "the constructor colors a new editor from the Dark palette before any theme change");
            if (args.Arguments == "--diff")
            {
                await DiffViewTests.RunAsync(log, core);
                core.Dispose();
                CompleteTests(log);
                return;
            }
            if (args.Arguments == "--syntax")
            {
                await SyntaxHighlightingTests.RunAsync(log, core);
                core.Dispose();
                CompleteTests(log);
                return;
            }
            if (args.Arguments == "--regex-memory")
            {
                const int documentBytes = 8 * 1024 * 1024;
                WriteDiagnostic("Regex allocation: preparing a bounded-line document");
                native.UndoCollection = false;
                native.WrapMode = WinUIEditor.Wrap.None;
                var allocationFixture = new StringBuilder(documentBytes);
                var allocationLine = new string('x', 1023) + "\r";
                for (var index = 0; index < documentBytes / allocationLine.Length; index++) allocationFixture.Append(allocationLine);
                native.SetText(allocationFixture.ToString());
                WriteDiagnostic("Regex allocation: warming the native search path");
                // Warm the API/cache before measuring its document view.
                await native.FindRegexAsync("\\A", true, 0, false, false, -1);
                await Task.Delay(50);
                var managedBefore = GC.GetTotalMemory(false);
                var nativeBefore = Windows.System.MemoryManager.AppMemoryUsage;
                var peakManaged = managedBefore;
                var peakNative = nativeBefore;
                var operation = native.FindRegexAsync("\\A", true, 0, false, false, -1);
                WriteDiagnostic("Regex allocation: observing native search completion");
                var completion = operation.AsTask();
                do
                {
                    peakManaged = Math.Max(peakManaged, GC.GetTotalMemory(false));
                    peakNative = Math.Max(peakNative, Windows.System.MemoryManager.AppMemoryUsage);
                    if (completion.IsCompleted) break;
                    await Task.Delay(1);
                } while (!completion.IsCompleted);
                var result = await completion;
                Check(result.Status == WinUIEditor.EditorSearchStatus.Found && result.Start == 0 && result.End == 0,
                    "native search over the allocation fixture uses full-subject anchors");
                core.Dispose();
                log.AppendLine($"PASS: native regex allocation observation; document={documentBytes} bytes, managed delta={peakManaged - managedBefore}, app memory delta={peakNative - nativeBefore}. This is not a maximum-size or frame-rate claim.");
                CompleteTests(log);
                return;
            }
            if (args.Arguments == "--search")
            {
                await CheckSearchBehaviorAsync(core);
                core.Dispose();
                log.AppendLine("PASS: V2 search parity and mutation admission while disabled or holding a native read lease.");
                CompleteTests(log);
                return;
            }
            if (args.Arguments == "--native-storage")
            {
                await CheckNativeUtf8ContractsAsync();
                await CheckNativeJournalContractsAsync();
                core.Dispose();
                log.AppendLine("PASS: native read leases, undo-preserving load, bounded journal checkpoints/replay/rotation.");
                CompleteTests(log);
                return;
            }
            native.GotoPos(0);
            Equal("", native.GetText(-1), "native text getter rejects a negative byte length");
            Equal("", native.GetCurLine(-1), "native line getter rejects a negative byte length");
            Equal("你", native.GetText(3), "native text getter uses the requested UTF-8 byte length");
            Equal("你", native.GetCurLine(3), "native line getter uses the requested UTF-8 byte length");
            native.SetRepresentation("😀", "Unicode representation 中文");
            Equal("Unicode representation 中文", native.GetRepresentation("😀"),
                "generated getter retains a converted Unicode key across both native calls");
            native.ClearRepresentation("😀");
            WriteDiagnostic("Initial text read: " + initialText.Length);
            Equal(normalized, initialText, "import/NUL/trailing newline");
            Check(!nativeView.MouseWheelZoomEnabled, "host owns percentage wheel zoom");
            var standaloneView = new WinUIEditor.EditorBaseControl();
            Check(standaloneView.MouseWheelZoomEnabled, "native wheel zoom stays enabled by default");
            await CheckNativeFinalizerReleaseAsync();
            await CheckDisposeReleasesDocumentAsync();
            log.AppendLine("PASS: V2 native control release from managed finalizers with a live UI dispatcher, and document release on Dispose.");
            await CheckNativeUtf8ContractsAsync();
            log.AppendLine("PASS: owned canonical UTF-8 stream loading, bounded native readers, edit leases, and atomic undo-preserving replacement.");
            await CheckNativeScrollWidthContractsAsync();
            log.AppendLine("PASS: native document, deletion, and font invalidation refresh tracked widths while preserving explicit fixed widths.");
            CheckNativeNotificationLifetime();
            log.AppendLine("PASS: borrowed native notification payloads expire safely while requested owned snapshots remain readable.");
            await CheckNativeJournalContractsAsync();
            log.AppendLine("PASS: bounded native edit journaling, durable immutable prefixes, atomic replay, imported continuation, and frozen save rotation.");
            var automationPeer = FrameworkElementAutomationPeer.CreatePeerForElement(nativeView);
            var textProvider = (ITextProvider)automationPeer.GetPattern(PatternInterface.Text);
            Check(textProvider != null, "native text automation pattern");
            var valueProvider = (IValueProvider)automationPeer.GetPattern(PatternInterface.Value);
            valueProvider.SetValue("automation😀\0tail");
            Equal("automation😀\0tail", core.GetText(), "automation replacement preserves embedded NUL");
            native.Undo();
            Equal(normalized, core.GetText(), "automation replacement is one undo action");
            native.ReadOnly = true;
            Check(valueProvider.IsReadOnly, "automation exposes native read-only state");
            var rejectedReadOnlyValue = false;
            try { valueProvider.SetValue("read-only replacement"); }
            catch (Exception) { rejectedReadOnlyValue = true; }
            Check(rejectedReadOnlyValue, "automation cannot replace a read-only document");
            native.ReadOnly = false;
            core.IsEnabled = false;
            Check(valueProvider.IsReadOnly, "automation exposes disabled editor state");
            var rejectedDisabledValue = false;
            try { valueProvider.SetValue("disabled replacement"); }
            catch (Exception) { rejectedDisabledValue = true; }
            Check(rejectedDisabledValue, "automation cannot edit during disabled document preparation");
            core.IsEnabled = true;
            Equal(normalized, core.GetText(), "rejected automation edits leave the document unchanged");
            var rangeBeforeEdit = textProvider.DocumentRange;
            Equal(normalized, rangeBeforeEdit.GetText(-1), "automation full text with embedded NUL");
            Equal(normalized, textProvider.DocumentRange.GetText(1024), "automation oversized text limit returns the full range");
            Equal("你", textProvider.DocumentRange.GetText(1), "automation Unicode character limit");
            Equal("你好😀", textProvider.DocumentRange.GetText(3), "automation UTF-8 boundaries");
            Equal("", textProvider.DocumentRange.GetText(0), "automation zero-length request");
            var characterRange = textProvider.DocumentRange;
            characterRange.ExpandToEnclosingUnit(Windows.UI.Xaml.Automation.Text.TextUnit.Character);
            Equal("你", characterRange.GetText(-1), "automation character expansion");
            Check(characterRange.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, 1) == 1,
                "automation character movement");
            Equal("好", characterRange.GetText(-1), "automation movement keeps UTF-8 boundaries");
            Check(characterRange.MoveEndpointByUnit(Windows.UI.Xaml.Automation.Text.TextPatternRangeEndpoint.End,
                Windows.UI.Xaml.Automation.Text.TextUnit.Character, 1) == 1,
                "automation endpoint movement");
            Equal("好😀", characterRange.GetText(-1), "automation endpoint keeps emoji intact");
            Equal("好😀", characterRange.GetText(1024), "automation oversized limit respects a nonzero range start");
            var normalizedMove = textProvider.DocumentRange;
            Check(normalizedMove.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, 1) == 1,
                "automation movement normalizes a multi-character range");
            Equal("好", normalizedMove.GetText(-1), "automation moved range contains one character");
            var endRange = textProvider.DocumentRange;
            Check(endRange.MoveEndpointByUnit(Windows.UI.Xaml.Automation.Text.TextPatternRangeEndpoint.Start,
                Windows.UI.Xaml.Automation.Text.TextUnit.Character, int.MaxValue) > 0,
                "automation endpoint stops at the document end");
            Equal("", endRange.GetText(-1), "automation endpoint at EOF is degenerate");
            Check(endRange.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, 1) == 0,
                "automation movement at EOF does not wrap to the start");
            endRange.ExpandToEnclosingUnit(Windows.UI.Xaml.Automation.Text.TextUnit.Character);
            Equal("", endRange.GetText(-1), "automation character expansion at EOF stays empty");
            var lastCharacter = textProvider.DocumentRange;
            Check(lastCharacter.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, int.MaxValue) > 0,
                "automation character range stops on the final character");
            Equal("\r", lastCharacter.GetText(-1), "automation end movement retains the final character");
            Check(lastCharacter.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, 1) == 0,
                "automation final character cannot move beyond EOF");
            Check(lastCharacter.Move(Windows.UI.Xaml.Automation.Text.TextUnit.Character, int.MinValue) < 0,
                "automation movement stops at the document start");
            Equal("你", lastCharacter.GetText(-1), "automation start movement keeps Unicode intact");
            core.SetTextSelectionPosition(-10, int.MaxValue);
            core.GetTextSelectionPosition(out var restoredStart, out var restoredEnd);
            Check(restoredStart == 0 && restoredEnd == normalized.Length,
                "restored selection clamps negative start and oversized end");
            Equal(normalized.Replace('\0', ' '), core.GetSelectedText(),
                "selected text uses the native clipboard policy for embedded NUL");
            core.SetTextSelectionPosition(int.MaxValue, int.MaxValue);
            core.GetTextSelectionPosition(out var clampedStart, out var clampedEnd);
            Check(clampedStart == normalized.Length && clampedEnd == normalized.Length,
                "restored caret beyond EOF stays at EOF");
            core.SetTextSelectionPosition(-10, -10);
            core.GetTextSelectionPosition(out clampedStart, out clampedEnd);
            Check(clampedStart == 0 && clampedEnd == 0, "restored negative caret stays at the start");
            core.SetTextSelectionPosition(2, 4);
            WriteDiagnostic("Initial selection set");
            var initialSelection = core.GetSelectedText();
            WriteDiagnostic("Initial selection read: " + initialSelection.Length);
            Equal("😀", initialSelection, "UTF-16 emoji selection");
            core.GetLineColumnSelection(out var startLine, out _, out var column, out _, out var selected, out var lines);
            WriteDiagnostic("Line metadata read");
            Check(startLine == 1 && column == 3 && selected == 2 && lines == 5, "line/column/count");
            core.MarkSaved();
            WriteDiagnostic("Marked saved");
            core.TypeText("替换");
            WriteDiagnostic("Selection replaced");
            Check(core.IsDocumentModified, "save point left");
            WriteDiagnostic("Before undo");
            core.Undo();
            WriteDiagnostic("After undo");
            Equal(normalized, core.GetText(), "undo");
            Check(!core.IsDocumentModified, "save point reached");
            core.Redo();
            WriteDiagnostic("After redo");
            Check(core.IsDocumentModified, "redo");
            core.MarkModified();
            core.ClearUndoQueue();
            Check(core.IsDocumentModified, "recovered dirty state survives history reset");
            WriteDiagnostic("After recovery save point checks");
            ThemeSettingsService.SetAccentColor(Windows.UI.Color.FromArgb(255, 210, 52, 56));
            await Task.Delay(50);
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionBack)) == 0xff3834d2,
                "active selection follows custom accent color");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveBack)) == 0xff3834d2,
                "Find result keeps custom accent color while editor loses focus");
            const string arabic = "مرحبا بالعالم 123 ABC";
            core.SetText(arabic);
            Equal(arabic, core.GetText(), "Arabic/mixed text round trip");
            core.SetTextSelectionPosition(0, 5);
            Equal("مرحبا", core.GetSelectedText(), "Arabic UTF-16 selection");
            Check(native.Bidirectional == WinUIEditor.Bidirectional.L2r,
                "native mixed-direction text support enabled");
            backdrop.FlowDirection = FlowDirection.RightToLeft;
            Check(core.FlowDirection == FlowDirection.LeftToRight && nativeView.FlowDirection == FlowDirection.LeftToRight,
                "RTL app chrome does not mirror native text or line numbers");
            backdrop.FlowDirection = FlowDirection.LeftToRight;
            await Task.Delay(100);
            Check(native.PointXFromPosition(0) > native.PointXFromPosition(2),
                "Arabic caret advances right-to-left without mirroring glyphs");
            const string mixed = "ABC مرحبا بالعالم 123 DEF";
            core.SetText(mixed);
            core.SetTextSelectionPosition(4, 9);
            Equal("مرحبا", core.GetSelectedText(), "Arabic word selected in mixed-direction text");
            await Task.Delay(100);
            var arabicStartX = native.PointXFromPosition(4);
            var arabicNextX = native.PointXFromPosition(6);
            Check(arabicStartX > arabicNextX, "mixed-direction Arabic caret order");
            var hit = native.PositionFromPointClose((arabicStartX + arabicNextX) / 2,
                native.PointYFromPosition(4) + (int)core.GetSingleLineHeight() / 2);
            Check(hit == 4 || hit == 6, "hit testing follows the rendered Arabic character");
            core.TypeText("أهلا");
            Equal("ABC أهلا بالعالم 123 DEF", core.GetText(), "Arabic replacement uses logical document order");
            core.Undo();
            Equal(mixed, core.GetText(), "mixed-direction replacement undo");
            if (args.Arguments == "--bidi-preview")
            {
                core.SetText("مرحبا بالعالم\rABC مرحبا بالعالم 123 DEF\rالعربية: 123 (ABC)\rעברית ABC 123\rمرحبا\tABC\rالسَّلَامُ عَلَيْكُمْ\rArabic: العربية — English: hello");
                core.SetTextSelectionPosition(17, 22);
                var reference = new TextBlock
                {
                    Text = "LTR paragraph reference:\nمرحبا بالعالم\nABC مرحبا بالعالم 123 DEF\nالعربية: 123 (ABC)\nעברית ABC 123\nمرحبا\tABC\nالسَّلَامُ عَلَيْكُمْ\nArabic: العربية — English: hello",
                    FontFamily = core.FontFamily,
                    FontSize = core.FontSize,
                    Foreground = new SolidColorBrush(Windows.UI.Colors.White),
                    Margin = new Thickness(40, 10, 0, 10),
                    FlowDirection = FlowDirection.LeftToRight
                };
                backdrop.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
                backdrop.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
                Grid.SetRow(reference, 1);
                backdrop.Children.Add(reference);
                core.Focus(FocusState.Programmatic);
                await Task.Delay(20000);
                backdrop.Children.Remove(reference);
                backdrop.RowDefinitions.Clear();
            }
            core.SetText("alpha bravo\rcharl delta\rechoo foxxx");
            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 0;
            native.RectangularSelectionCaret = 27;
            Check(native.Selections == 3, "rectangular selection creates one range per row");
            CheckEditorPalette();
            ThemeSettingsService.SetTheme(ElementTheme.Light);
            await Task.Delay(100);
            Check((native.StyleGetFore(DefaultStyle) & 0xffffff) == 0x000000,
                "light theme uses dark editor text");
            Check((native.StyleGetFore(LineNumberStyle) & 0xffffff) == 0x696969,
                "light theme keeps line numbers dim");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.CaretLineBack)) == 0x18000000,
                "light theme line highlight comes from theme resources");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionText)) == 0xffffffff &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveText)) == 0xffffffff,
                "light theme selections and inactive Find results use white text");
            CheckSelectionPalette(native, 0xff3834d2);
            ThemeSettingsService.SetTheme(ElementTheme.Dark);
            await Task.Delay(100);
            Check((native.StyleGetFore(DefaultStyle) & 0xffffff) == 0xffffff,
                "dark theme restores light editor text");
            Check((native.StyleGetFore(LineNumberStyle) & 0xffffff) == 0x969696 &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.CaretLineBack)) == 0x18ffffff,
                "dark theme line numbers and line highlight come from theme resources");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveBack)) == 0xff3834d2,
                "theme switches preserve inactive selection accent");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionText)) == 0xffffffff &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveText)) == 0xffffffff,
                "theme switches preserve white selected text");
            Check(native.Selections == 3, "theme change preserves rectangular ranges");
            CheckSelectionPalette(native, 0xff3834d2);
            ThemeSettingsService.SetAccentColor(Windows.UI.Colors.DodgerBlue);
            await Task.Delay(50);
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionBack)) == 0xffff901e &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveBack)) == 0xffff901e,
                "live accent change updates active and inactive selections");
            Check(unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionText)) == 0xffffffff &&
                unchecked((uint)native.GetElementColour(WinUIEditor.Element.SelectionInactiveText)) == 0xffffffff,
                "live accent change preserves white selected text");
            CheckSelectionPalette(native, 0xffff901e);
            native.SelectionMode = WinUIEditor.SelectionMode.Stream;
            native.UndoCollection = false;
            WriteDiagnostic("Second reset: undo disabled");
            native.ClearAll();
            WriteDiagnostic("Second reset: cleared");
            native.UndoCollection = true;
            Check(core.IsDocumentEmpty, "clear existing Unicode text with undo disabled");
            Equal("", textProvider.DocumentRange.GetText(-1), "automation empty document");
            Equal("", rangeBeforeEdit.GetText(-1), "automation range after document reset");
            Check(native.CaretLineLayer == WinUIEditor.Layer.UnderText, "translucent line highlight layer");
            core.DisplayLineHighlighter = false;
            Check(!native.CaretLineVisible, "line highlight disabled");
            core.DisplayLineHighlighter = true;
            Check((uint)native.GetElementColour(WinUIEditor.Element.CaretLineBack) >> 24 == 24, "line highlight alpha after toggling");
            core.SetText("中文 test TEST tester\r中文");
            WriteDiagnostic("Search text loaded");
            core.SetTextSelectionPosition(0, 0);
            WriteDiagnostic("Search selection reset");
            Check(((await core.FindAsync(new SearchContext("中文"), false, false)).Status == WinUIEditor.EditorSearchStatus.Found), "Unicode search");
            WriteDiagnostic("After Unicode search");
            Equal("中文", core.GetSelectedText(), "Unicode search selection");
            Check(((await core.ReplaceAllAsync(new SearchContext("test", matchWholeWord: true), "😀")).Status == WinUIEditor.EditorSearchStatus.Found), "whole word replace all");
            WriteDiagnostic("After literal replacement");
            Equal("中文 😀 😀 tester\r中文", core.GetText(), "whole word/case folding");
            core.Undo();
            Equal("中文 test TEST tester\r中文", core.GetText(), "replace all is one undo action");
            core.SetText("a1\ra2");
            Check(((await core.ReplaceAllAsync(new SearchContext("^a(\\d)$", useRegex: true), "$1😀")).Status == WinUIEditor.EditorSearchStatus.Found), "regex replacement");
            WriteDiagnostic("After regex replacement");
            var regexReplacementText = core.GetText();
            WriteDiagnostic("Regex replacement text read");
            Equal("1😀\r2😀", regexReplacementText, ".NET regex captures/multiline");
            WriteDiagnostic("Starting search behavior checks");
            await CheckSearchBehaviorAsync(core);
            WriteDiagnostic("Search behavior checks completed");
            log.AppendLine("PASS: case-sensitive search, forward/backward wrap, whole-word boundaries, single replacement, regex captures/escapes, and invalid regex.");
            await CheckLoadedIndentationAsync(native);
            CheckNativeEditingCommands(core, native);
            WriteDiagnostic("Native editing command checks completed");
            log.AppendLine("PASS: native indentation/dedentation, reversed and rectangular selections, line-boundary exclusion, Unicode line joining, and grouped undo.");
            var selectionEvents = 0;
            core.SelectionChanged += (_, __) => selectionEvents++;
            core.SetText("selected text");
            core.SelectAll();
            await Task.Delay(50);
            selectionEvents = 0;
            core.TypeText("replacement");
            await Task.Delay(100);
            Check(!core.HasSelection && selectionEvents > 0, "selection status updates after replacing selected text");
            core.DisplayLineNumbers = false;
            Check(native.GetMarginWidthN(0) == 0 && lineNumberReveal.Visibility == Visibility.Collapsed,
                "line numbers and hover disappear together");
            core.DisplayLineNumbers = true;
            Check(native.GetMarginWidthN(0) > 0 && lineNumberReveal.Visibility == Visibility.Visible,
                "line numbers and hover return together");
            var horizontalBar = FindNamedDescendant<ScrollBar>(nativeView, "HorizontalScrollBar");
            Check(horizontalBar != null, "horizontal scrollbar template part");
            core.SetText("short");
            await Task.Delay(100);
            Check(horizontalBar.Visibility == Visibility.Collapsed, "short line hides horizontal scrollbar");
            core.SetText(new string('x', 800));
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Visible, "overflow shows horizontal scrollbar");
            native.XOffset = 140;
            var previousXOffset = native.XOffset;
            var previousForeground = native.StyleGetFore(DefaultStyle);
            native.StyleSetFore(DefaultStyle, previousForeground ^ 0x00FFFFFF);
            native.StyleSetFore(DefaultStyle, previousForeground);
            native.StyleClearAll();
            nativeView.SetBackgroundColor(Windows.UI.Colors.Transparent);
            native.FontQuality = native.FontQuality;
            await Task.Delay(100);
            Check(previousXOffset > 0 && native.XOffset == previousXOffset,
                "color-only theme updates preserve the horizontal viewport after repaint");
            native.DeleteRange(0, 1);
            Check(horizontalBar.Visibility == Visibility.Visible && native.XOffset == previousXOffset,
                "deleting text keeps the horizontal scrollbar and viewport");
            await Task.Delay(150);
            Check(horizontalBar.Visibility == Visibility.Visible && native.XOffset == previousXOffset,
                "re-measuring after a deletion keeps the viewport while text still overflows");
            native.DeleteRange(0, native.Length - 5);
            await Task.Delay(150);
            Check(native.XOffset == previousXOffset, "re-measuring after an edit never moves the horizontal viewport");
            // At column 0, styling the lines below a long line must not shrink the width to theirs.
            core.SetSyntaxLanguage(DocumentLanguages.Find("csharp"), "sample.cs");
            await core.LoadTextAsync(new string('x', 800) + new StringBuilder().Insert(0, "\rint value = 1;", 400));
            await Task.Delay(150);
            var longLineWidth = native.ScrollWidth;
            native.FirstVisibleLine = 200;
            await Task.Delay(150);
            Check(native.FirstVisibleLine == 200 && native.XOffset == 0 && native.ScrollWidth >= longLineWidth &&
                horizontalBar.Visibility == Visibility.Visible, "paging a highlighted file past its long line keeps the horizontal scrollbar");
            core.SetSyntaxLanguage(DocumentLanguages.Find("python"), "sample.py");
            await Task.Delay(150);
            Check(native.ScrollWidth >= longLineWidth && horizontalBar.Visibility == Visibility.Visible,
                "a language reinstall that restyles the visible lines keeps the horizontal scrollbar");
            core.SetSyntaxLanguage(DocumentLanguages.PlainText, "sample.txt");
            core.SetText(new string('x', 800));
            await Task.Delay(100);
            var imageTarget = FindNamedDescendant<Border>(nativeView, "ImageTarget");
            Check(imageTarget != null && horizontalBar.ActualHeight > 0, "horizontal scrollbar laid out");
            var textBottom = imageTarget.TransformToVisual(nativeView)
                .TransformPoint(new Windows.Foundation.Point(0, imageTarget.ActualHeight)).Y;
            var barTop = horizontalBar.TransformToVisual(nativeView)
                .TransformPoint(new Windows.Foundation.Point()).Y;
            Check(textBottom <= barTop + 0.5, "horizontal scrollbar does not cover the text surface");
            core.TextWrapping = TextWrapping.Wrap;
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Collapsed, "Word Wrap hides horizontal scrolling");
            core.TextWrapping = TextWrapping.NoWrap;
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Visible, "disabling Word Wrap restores horizontal overflow");
            Check(Math.Abs(horizontalBar.Margin.Left - lineNumberReveal.Width) < 1,
                "horizontal scrollbar starts after line numbers");
            var scale = OperatingSystem.IsWindowsVersionAtLeast(10, 0, 18362)
                ? nativeView.XamlRoot.RasterizationScale
                : DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
            var additionalMargin = (int)Math.Round(7 * scale);
            native.SetMarginWidthN(1, additionalMargin);
            Check(Math.Abs(horizontalBar.Margin.Left -
                (native.GetMarginWidthN(0) + additionalMargin) / scale) < 1,
                "native scrollbar follows all fixed margins synchronously");
            native.SetMarginWidthN(1, 0);
            core.SetFontZoomFactor(110);
            Check(Math.Abs(horizontalBar.Margin.Left - lineNumberReveal.Width) < 1,
                "horizontal scrollbar follows zoomed line numbers");
            core.SetFontZoomFactor(100);
            await Task.Delay(50);
            var defaultLineHeight = core.GetSingleLineHeight();
            core.SetFontZoomFactor(510);
            Check(core.GetFontZoomFactor() == 500 && native.StyleGetSizeFractional(DefaultStyle) == 5250,
                "zoom clamps at 500 percent with matching native font size");
            core.SetFontZoomFactor(500);
            Check(!nativeView.MouseWheelZoomEnabled && native.Zoom == 0 && core.GetFontZoomFactor() == 500 &&
                native.StyleGetSizeFractional(DefaultStyle) == 5250,
                "repeated upper-limit zoom keeps the application's percentage font size");
            core.SetFontZoomFactor(5);
            Check(core.GetFontZoomFactor() == 10 && native.StyleGetSizeFractional(DefaultStyle) == 105,
                "zoom clamps at 10 percent with matching native font size");
            core.SetFontZoomFactor(10);
            Check(native.Zoom == 0 && core.GetFontZoomFactor() == 10 && native.StyleGetSizeFractional(DefaultStyle) == 105,
                "repeated lower-limit zoom keeps the application's percentage font size");
            core.FontSize = 1400;
            Check(core.GetFontZoomFactor() == 500 && core.FontSize == 70,
                "direct font-size changes respect the zoom bounds");
            core.SetFontZoomFactor(100);
            await Task.Delay(50);
            Check(native.Zoom == 0 && core.GetFontZoomFactor() == 100 &&
                native.StyleGetSizeFractional(DefaultStyle) == 1050 && core.GetSingleLineHeight() == defaultLineHeight,
                "reset restores actual default font metrics after percentage zoom");
            await Task.Delay(150);
            Check(horizontalBar.Visibility == Visibility.Visible,
                "reset keeps horizontal scrolling when text still overflows");
            core.DisplayLineNumbers = false;
            Check(horizontalBar.Margin.Left == 0, "horizontal scrollbar reaches left edge without line numbers");
            core.DisplayLineNumbers = true;
            core.SelectAll();
            core.TypeText("short");
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Collapsed, "deleting overflow hides horizontal scrollbar");
            core.SetText(new string('x', 50));
            await Task.Delay(100);
            Check(horizontalBar.Visibility == Visibility.Collapsed, "zoom-reset fixture fits at default font size");
            core.SetFontZoomFactor(500);
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Visible, "enlarged font shows horizontal overflow");
            native.XOffset = 100;
            Check(native.XOffset > 0, "enlarged text can be scrolled horizontally");
            core.SetFontZoomFactor(100);
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Collapsed && native.XOffset == 0,
                "zoom reset hides scrollbar and clears offset when text fits again");
            core.SetText(new string('x', 800));
            core.SetFontZoomFactor(300);
            await Task.Delay(200);
            var zoomedWidth = native.ScrollWidth;
            native.XOffset = zoomedWidth;
            core.SetFontZoomFactor(100);
            await Task.Delay(200);
            Check(horizontalBar.Visibility == Visibility.Visible && native.XOffset > 0 && native.XOffset < native.ScrollWidth &&
                native.ScrollWidth < zoomedWidth, "zooming out while scrolled right keeps text in view");
            native.XOffset = 0;
            core.SetFontZoomFactor(500);
            core.TextWrapping = TextWrapping.Wrap;
            await Task.Delay(200);
            Check(native.WrapCount(0) > 2, "single document line wraps into multiple display rows");
            native.FirstVisibleLine = 2;
            await Task.Delay(50);
            Check(native.FirstVisibleLine >= 2, "wrapped document is scrolled below its first display row");
            var wrappedRanges = textProvider.GetVisibleRanges();
            Check(wrappedRanges.Length == 1 && wrappedRanges[0].GetText(1024).Length > 0,
                "automation maps wrapped display rows to valid document ranges");
            native.FirstVisibleLine = 0;
            core.SetFontZoomFactor(100);
            core.TextWrapping = TextWrapping.NoWrap;
            using (var neverShown = new TextEditorCore())
            {
                neverShown.SetScrollViewerInitPosition(12, 345);
                neverShown.GetScrollViewerPosition(out var pendingHorizontal, out var pendingVertical);
                Check(pendingHorizontal == 12 && pendingVertical == 345, "a tab that was never shown keeps its restored scroll offsets");
            }
            log.AppendLine("PASS: normalization, NUL, UTF-16, line index, BiDi caret/hit testing/replacement, Unicode search, undo/save points, recovery, UI Automation text ranges, selection colors, zoom limits/reset, never-shown scroll offsets.");

            await core.LoadTextAsync("first😀\r\nsecond中文\nthird");
            Equal("first😀\rsecond中文\rthird", core.GetText(), "async loading applies V2 newline/Unicode normalization");
            Check(core.GoTo(3), "Go To navigates to the requested line");
            core.GetLineColumnSelection(out startLine, out _, out column, out _, out _, out lines);
            Check(startLine == 3 && column == 1 && lines == 3, "Go To reports the expected V2 line and column");
            Check(!core.GoTo(0) && !core.GoTo(4), "Go To rejects out-of-range lines");
            await core.LoadTextAsync("再次加载😀\r\nend");
            Equal("再次加载😀\rend", core.GetText(), "reload resets the V2 document correctly");
            Check((native.LineCharacterIndex & WinUIEditor.LineCharacterIndexType.Utf16) != 0,
                "native document attachment retains the UTF-16 index");
            var textBeforeCancellation = core.GetText();
            using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("cancelled candidate 😀\rsecond line")))
            using (var overlap = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("overlapping candidate")))
            using (var input = stream.GetInputStreamAt(0))
            using (var overlapInput = overlap.GetInputStreamAt(0))
            {
                var canceledLoad = native.LoadUtf8Async(input, stream.Size, false);
                // Cancel() reports Canceled at once; this task waits until the worker stops reading the stream.
                var canceledCompletion = canceledLoad.AsTask();
                bool rejectedOverlap = false;
                try { await native.LoadUtf8Async(overlapInput, overlap.Size, false); }
                catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedOverlap = true; }
                Check(rejectedOverlap, "native loader rejects overlapping operations on one editor");
                canceledLoad.Cancel();
                bool observedCancellation = false;
                try { await canceledCompletion; }
                catch (OperationCanceledException) { observedCancellation = true; }
                Check(observedCancellation, "native loader reports cancellation");
            }
            Equal(textBeforeCancellation, core.GetText(), "cancelled and overlapping loads keep the existing document");
            await core.LoadTextAsync("after cancellation😀\r\nend");
            Equal("after cancellation😀\rend", core.GetText(), "native loader restarts immediately after cancellation and retired loads cannot overwrite it");
            log.AppendLine("PASS: V2 async load/reload, Go To boundaries, Word Wrap, and scrollbar layout.");
            core.RequestedTheme = ElementTheme.Dark;
            core.SetFontZoomFactor(110);
            core.SetFontZoomFactor(100);
            Check(native.CaretLineLayer == WinUIEditor.Layer.UnderText, "highlight after theme/font changes");
            Check(lineNumberReveal.BorderThickness.Right == 0, "line-number border remains hidden after theme/font changes");
            await CheckThemeAppliesOnlyOnChangeAsync(backdrop, core, native);
            log.AppendLine("PASS: editor colors from theme resources; tab re-entry, repeated theme notifications and unchanged font sizes keep applied styles.");
            await CheckDialogThemeResourcesAsync();
            log.AppendLine("PASS: dialog background and hyperlink colors follow a dialog theme that differs from the application theme.");
            // Runs before the syntax tests, which configure the native lexer directly.
            await IndentationTests.RunAsync(log, core);
            core.Focus(FocusState.Programmatic);
            if (args.Arguments == "--preview") await Task.Delay(20000);
            await SyntaxHighlightingTests.RunAsync(log, core);
            await DiffViewTests.RunAsync(log, core);
            core.Dispose();
            await SessionStorageTests.RunAsync(log);
            await DocumentBaselineTests.RunAsync(log);
            await SessionRecoveryTests.RunAsync(log);
            await SessionServiceTests.RunAsync(log);
            await SessionRegistryTests.RunAsync(log);
            await SessionControllerTests.RunAsync(log);
            await DocumentFileWriterTests.RunAsync(log);
        }
        catch (Exception ex) { WriteDiagnostic(ex.ToString()); log.AppendLine("FAIL: " + ex); }
        CompleteTests(log);
    }

    private void CompleteTests(StringBuilder log)
    {
        System.IO.File.WriteAllText(System.IO.Path.Combine(ApplicationData.Current.LocalFolder.Path, "results.txt"), log.ToString());
        Exit();
    }

    private static void Check(bool condition, string test) { if (!condition) throw new Exception(test); }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference[] CreateNativeFinalizerFixtures()
    {
        var plain = new WinUIEditor.EditorBaseControl();
        var edited = new WinUIEditor.EditorBaseControl();
        edited.Editor.PasteText("finalizer resources😀");
        return [new WeakReference(plain), new WeakReference(edited)];
    }

    private static async Task<InMemoryRandomAccessStream> CreateUtf8StreamAsync(byte[] bytes)
    {
        var stream = new InMemoryRandomAccessStream();
        try
        {
            await stream.WriteAsync(bytes.AsBuffer());
            stream.Seek(0);
            return stream;
        }
        catch { stream.Dispose(); throw; }
    }

    private static string ReadUtf8Buffer(IBuffer buffer)
    {
        var bytes = new byte[buffer.Length];
        using (var reader = DataReader.FromBuffer(buffer)) reader.ReadBytes(bytes);
        return Encoding.UTF8.GetString(bytes);
    }

    private static async Task CheckNativeUtf8ContractsAsync()
    {
        // One chunk boundary is enough to verify the streaming contract;
        // this fixture makes no engine size/performance claim.
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        var canonical = new string('a', 65535) + "😀\0tail\r中文";
        var bytes = Encoding.UTF8.GetBytes(canonical);
        using (var stream = await CreateUtf8StreamAsync(bytes))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)bytes.Length, false);
        Equal(canonical, editor.GetText(editor.Length + 1), "canonical streaming handles a split UTF-8 scalar and embedded NUL");
        Check(editor.EOLMode == WinUIEditor.EndOfLine.Cr, "canonical stream loading owns the native CR editing policy");
        Equal("a", ReadUtf8Buffer(editor.ReadUtf8Range(65534, 4)), "bounded reads trim a partial UTF-8 scalar");
        Equal("😀", ReadUtf8Buffer(editor.ReadUtf8Range(65535, 4)), "bounded reads admit one complete four-byte scalar");
        var ownedChunk = editor.ReadUtf8Range(65535, 4);
        Equal("\0tai", ReadUtf8Buffer(editor.ReadUtf8Range(65539, 4)), "bounded reads preserve NUL bytes");
        Check(editor.ReadUtf8Range(editor.Length, 4).Length == 0, "bounded read returns an empty EOF buffer");
        var rejectedBoundary = false;
        try { editor.ReadUtf8Range(65536, 4); }
        catch (ArgumentException) { rejectedBoundary = true; }
        Check(rejectedBoundary, "bounded read rejects a continuation-byte offset");
        using (var reader = editor.AcquireUtf8Reader())
        {
            Check(reader.Length == (ulong)bytes.Length && editor.ReadOnly, "native reader pins length and freezes edits");
            editor.SetText("blocked mutation");
            Equal(canonical, editor.GetText(editor.Length + 1), "native read lease rejects ordinary text mutation");
            var rejectedUnfreeze = false;
            try { editor.ReadOnly = false; }
            catch (Exception) { rejectedUnfreeze = true; }
            Check(rejectedUnfreeze, "native read lease rejects programmatic unfreeze");
            var rejectedPointer = false;
            try { fixture.SendMessage(WinUIEditor.ScintillaMessage.GetCharacterPointer, 0, 0); }
            catch (Exception) { rejectedPointer = true; }
            Check(rejectedPointer, "native read lease rejects gap-moving raw pointers");
            editor.GotoPos(65539);
            Check(editor.CurrentPos == 65539, "native reader permits navigation");
            var fromWorker = await Task.Run(async () => ReadUtf8Buffer(await reader.ReadAsync(65535, 4)));
            Equal("😀", fromWorker, "reader marshals an async worker read to its native owner");
            using (var secondReader = editor.AcquireUtf8Reader()) secondReader.Dispose();
            Check(editor.ReadOnly, "one remaining reader keeps edits frozen");
        }
        Check(!editor.ReadOnly, "last reader restores the prior writable state");
        editor.ReadOnly = true;
        using (var readOnlyReader = editor.AcquireUtf8Reader()) { }
        Check(editor.ReadOnly, "reader restores a prior read-only document");
        editor.ReadOnly = false;
        foreach (var invalid in new[] { Encoding.UTF8.GetBytes("not\ncanonical"), [0xf0, 0x9f], [0xed, 0xa0, 0x80] })
        {
            var rejected = false;
            using (var stream = await CreateUtf8StreamAsync(invalid))
            using (var input = stream.GetInputStreamAt(0))
            {
                try { await editor.LoadUtf8Async(input, (ulong)invalid.Length, false); }
                catch (ArgumentException) { rejected = true; }
            }
            Check(rejected, "native stream import rejects noncanonical or malformed UTF-8");
            Equal(canonical, editor.GetText(editor.Length + 1), "invalid stream import preserves the previous document");
        }
        using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("source")))
        using (var input = stream.GetInputStreamAt(0))
        {
            var rejectedCap = false;
            try { await editor.LoadUtf8Async(input, (ulong)int.MaxValue + 1, false); }
            catch (ArgumentException) { rejectedCap = true; }
            Check(rejectedCap && stream.Position == 0, "stream admission rejects Int32 overflow before reading");
        }
        editor.SetSel(0, 1);
        editor.PasteText("B");
        Equal("😀", ReadUtf8Buffer(ownedChunk), "owned byte buffers survive subsequent document edits");
        var edited = "B" + canonical.Substring(1);
        var replacement = Encoding.UTF8.GetBytes("replacement😀\0tail\rfinal");
        var rejectedTruncatedStream = false;
        using (var stream = await CreateUtf8StreamAsync(replacement))
        using (var input = stream.GetInputStreamAt(0))
        {
            try { await editor.LoadUtf8Async(input, (ulong)replacement.Length + 1, true); }
            catch (Exception ex) when (ex.HResult == unchecked((int)0x80070026)) { rejectedTruncatedStream = true; }
        }
        Check(rejectedTruncatedStream, "native replacement rejects a source truncated during preparation");
        Equal(edited, editor.GetText(editor.Length + 1), "failed stream preparation preserves the complete original text and undo owner");
        var rejectedCallbackAllocate = false;
        var rejectedCallbackUndoReset = false;
        WinUIEditor.ModifiedHandler transactionCallback = (sender, modification) =>
        {
            if ((modification.ModificationType & (int)WinUIEditor.ModificationFlags.BeforeDelete) == 0) return;
            try { sender.Allocate(sender.Length + 1048576); }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedCallbackAllocate = true; }
            try { sender.EmptyUndoBuffer(); }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedCallbackUndoReset = true; }
        };
        editor.Modified += transactionCallback;
        using (var stream = await CreateUtf8StreamAsync(replacement))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)replacement.Length, true);
        editor.Modified -= transactionCallback;
        Check(rejectedCallbackAllocate && rejectedCallbackUndoReset, "atomic replacement rejects callback reallocation and undo-history destruction");
        Equal("replacement😀\0tail\rfinal", editor.GetText(editor.Length + 1), "undo-preserving stream replacement commits counted bytes");
        editor.Undo();
        Equal(edited, editor.GetText(editor.Length + 1), "one undo reverses the whole stream replacement");
        editor.Undo();
        Equal(canonical, editor.GetText(editor.Length + 1), "replacement retains prior native undo history");
        editor.Redo();
        editor.Redo();
        Equal("replacement😀\0tail\rfinal", editor.GetText(editor.Length + 1), "replacement history supports redo including NUL");
        using (var stream = await CreateUtf8StreamAsync(bytes))
        using (var input = stream.GetInputStreamAt(0))
        using (var nextStream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("successor")))
        using (var nextInput = nextStream.GetInputStreamAt(0))
        {
            var canceled = editor.LoadUtf8Async(input, (ulong)bytes.Length, true);
            canceled.Cancel();
            try { await canceled; }
            catch (OperationCanceledException) { }
            await editor.LoadUtf8Async(nextInput, 9, true);
        }
        Equal("successor", editor.GetText(editor.Length + 1), "canceled stream generation cannot publish over its successor");
        var ansiFixture = new WinUIEditor.EditorBaseControl();
        var ansiEditor = ansiFixture.Editor;
        ansiEditor.CodePage = 0;
        ansiEditor.SetText("unchanged");
        var rejectedAnsiRead = false;
        try { ansiEditor.ReadUtf8Range(0, 4); }
        catch (ArgumentException) { rejectedAnsiRead = true; }
        var rejectedAnsiLease = false;
        try { using (var reader = ansiEditor.AcquireUtf8Reader()) { } }
        catch (ArgumentException) { rejectedAnsiLease = true; }
        var rejectedAnsiReplace = false;
        using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("canonical")))
        using (var input = stream.GetInputStreamAt(0))
        {
            try { await ansiEditor.LoadUtf8Async(input, 9, true); }
            catch (ArgumentException) { rejectedAnsiReplace = true; }
        }
        Check(rejectedAnsiRead && rejectedAnsiLease && rejectedAnsiReplace, "UTF-8 range, lease, and undo-preserving APIs reject an ANSI document");
        Equal("unchanged", ansiEditor.GetText(ansiEditor.Length + 1), "UTF-8 admission leaves an ANSI document unchanged");
        GC.KeepAlive(fixture);
        GC.KeepAlive(ansiFixture);
    }

    private static async Task CheckColdNativeFactoryAsync()
    {
        // A missing isolated file proves that cold activation reached native
        // validation. No editor, instance checkpoint, or factory cache exists.
        var path = System.IO.Path.Combine(ApplicationData.Current.LocalFolder.Path,
            "cold-journal-factory-" + Guid.NewGuid().ToString("N") + ".npj");
        var reachedNativeFileAccess = false;
        try { await WinUIEditor.EditorJournalCheckpoint.OpenAsync(path, 0, 0, 48, new string('0', 64)); }
        catch (Exception ex) when (ex.HResult == unchecked((int)0x80070002)) { reachedNativeFileAccess = true; }
        Check(reachedNativeFileAccess, "cold static factory resolves through the package registration");
    }

    private static async Task CheckNativeScrollWidthContractsAsync()
    {
        // Exercise the component directly, without a host width reset.
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        editor.ScrollWidthTracking = true;
        editor.ScrollWidth = 9000;
        var bytes = Encoding.UTF8.GetBytes("original\rnext");
        using (var stream = await CreateUtf8StreamAsync(bytes))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)bytes.Length, false);
        Check(editor.ScrollWidth < 9000, "document attachment retires the previous tracked pixel width");
        editor.ScrollWidth = 9000;
        editor.SelectAll();
        editor.Clear();
        Check(editor.ScrollWidth == 9000, "native deletion keeps the published width until it is measured again");
        await Task.Delay(150);
        Check(editor.ScrollWidth == 1, "native deletion re-measures tracked width even without a host modification handler");
        editor.PasteText("editable");
        editor.ScrollWidth = 9000;
        editor.StyleSetSizeFractional(DefaultStyle, editor.StyleGetSizeFractional(DefaultStyle) + 100);
        await Task.Delay(150);
        Check(editor.ScrollWidth < 9000, "native font-size changes retire stale tracked metrics");
        editor.StyleClearAll();
        editor.ScrollWidth = 9000;
        editor.XOffset = 140;
        var previousOffset = editor.XOffset;
        editor.StyleSetFore(0, 0x102030);
        editor.StyleSetBack(0, 0x405060);
        editor.SetElementColour(WinUIEditor.Element.SelectionBack, unchecked((int)0xFF123456));
        editor.StyleClearAll();
        editor.FontQuality = editor.FontQuality;
        Check(editor.ScrollWidth == 9000 && editor.XOffset == previousOffset,
            "color-only styles, selection colors, and identical fonts preserve tracked width and horizontal position");
        Check((editor.DocumentOptions & WinUIEditor.DocumentOption.StylesNone) != 0,
            "canonical plain-text stream fixture omits per-byte style storage");
        var stylingFixture = new WinUIEditor.EditorBaseControl();
        var stylingEditor = stylingFixture.Editor;
        Check((stylingEditor.DocumentOptions & WinUIEditor.DocumentOption.StylesNone) == 0,
            "token-width fixture has native style storage");
        stylingEditor.PasteText("editable");
        stylingEditor.ScrollWidthTracking = true;
        stylingEditor.StyleSetSizeFractional(1, 3000);
        stylingEditor.StyleSetSizeFractional(2, 800);
        stylingEditor.StartStyling(0, 0);
        stylingEditor.SetStyling(stylingEditor.Length, 1);
        Check(stylingEditor.GetStyleAt(0) == 1, "token-width fixture applies the wide-font style");
        stylingEditor.ScrollWidth = 9000;
        stylingEditor.StartStyling(0, 0);
        stylingEditor.SetStyling(stylingEditor.Length, 2);
        await Task.Delay(150);
        Check(stylingEditor.GetStyleAt(0) == 2 && stylingEditor.ScrollWidth < 9000,
            "changing token runs from a wide font to a narrow font retires tracked metrics");
        stylingEditor.ScrollWidth = 9000;
        stylingEditor.StyleClearAll();
        await Task.Delay(150);
        Check(stylingEditor.ScrollWidth < 9000, "clearing styles with different font metrics retires tracked width");
        editor.ScrollWidth = 9000;
        var replacement = Encoding.UTF8.GetBytes("replacement");
        using (var stream = await CreateUtf8StreamAsync(replacement))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)replacement.Length, true);
        Check(editor.ScrollWidth < 9000, "undo-preserving publication resets tracked width on the existing document");
        editor.ScrollWidth = 9000;
        editor.SelectAll();
        editor.Clear();
        editor.ScrollWidth = 9000;
        await Task.Delay(150);
        Check(editor.ScrollWidth == 9000, "a fixed width cancels a pending re-measure from the tracking policy");

        editor.ScrollWidthTracking = false;
        editor.ScrollWidth = 9000;
        editor.SelectAll();
        editor.Clear();
        Check(editor.ScrollWidth == 9000, "native deletion preserves an explicitly fixed scroll width");
        editor.StyleSetSizeFractional(DefaultStyle, 1100);
        Check(editor.ScrollWidth == 9000, "font invalidation preserves an explicitly fixed scroll width");
        using (var stream = await CreateUtf8StreamAsync(bytes))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)bytes.Length, false);
        Check(editor.ScrollWidth == 9000, "document attachment preserves fixed scroll-width policy");
        using (var stream = await CreateUtf8StreamAsync(replacement))
        using (var input = stream.GetInputStreamAt(0))
            await editor.LoadUtf8Async(input, (ulong)replacement.Length, true);
        Check(editor.ScrollWidth == 9000, "undo-preserving publication preserves fixed scroll-width policy");
        editor.ScrollWidthTracking = true;
        Check(editor.ScrollWidth == 1, "enabling intrinsic tracking retires metrics from the previous fixed-width policy");
        GC.KeepAlive(fixture);
        GC.KeepAlive(stylingFixture);
    }

    private static void CheckNativeNotificationLifetime()
    {
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        editor.CodePage = (int)WinUIEditor.EditorConstants.ScCpUtf8;
        WinUIEditor.ModifiedEventArgs borrowed = null, owned = null;
        WinUIEditor.ModifiedHandler borrowHandler = (sender, modification) =>
        {
            if ((modification.ModificationType & (int)WinUIEditor.ModificationFlags.InsertText) != 0) borrowed = modification;
        };
        editor.Modified += borrowHandler;
        editor.PasteText("borrowed😀\0tail");
        editor.Modified -= borrowHandler;
        Check(borrowed != null && borrowed.Length > 0, "notification metadata remains readable after dispatch");
        var rejectedLateText = false;
        try { var late = borrowed.Text; }
        catch (Exception ex) when (ex is ObjectDisposedException || ex.HResult == ObjectClosed) { rejectedLateText = true; }
        var rejectedLateBuffer = false;
        try { var late = borrowed.TextAsBuffer; }
        catch (Exception ex) when (ex is ObjectDisposedException || ex.HResult == ObjectClosed) { rejectedLateBuffer = true; }
        Check(rejectedLateText && rejectedLateBuffer, "unmaterialized borrowed payload cannot be dereferenced after its native notification");
        WinUIEditor.ModifiedHandler ownHandler = (sender, modification) =>
        {
            if ((modification.ModificationType & (int)WinUIEditor.ModificationFlags.InsertText) == 0) return;
            Equal("owned😀\0tail", modification.Text, "explicit notification text materialization preserves NUL");
            Equal("owned😀\0tail", ReadUtf8Buffer(modification.TextAsBuffer), "explicit notification byte materialization preserves NUL");
            owned = modification;
        };
        editor.Modified += ownHandler;
        editor.SelectAll();
        editor.PasteText("owned😀\0tail");
        editor.Modified -= ownHandler;
        editor.SelectAll();
        editor.PasteText("retire native storage");
        Equal("owned😀\0tail", owned.Text, "materialized notification text survives subsequent native edits");
        Equal("owned😀\0tail", ReadUtf8Buffer(owned.TextAsBuffer), "materialized notification bytes survive subsequent native edits");
        var rejectedUndoRetirement = false;
        var rejectedMidEditReader = false;
        var observedCountedPayload = false;
        const string callbackText = "callback😀\0tail";
        WinUIEditor.ModifiedHandler retireHandler = (sender, modification) =>
        {
            if ((modification.ModificationType & (int)WinUIEditor.ModificationFlags.InsertText) == 0) return;
            try { editor.EmptyUndoBuffer(); }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedUndoRetirement = true; }
            try { using (var midEdit = editor.AcquireUtf8Reader()) { } }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedMidEditReader = true; }
        };
        WinUIEditor.ModifiedHandler inspectHandler = (sender, modification) =>
        {
            if ((modification.ModificationType & (int)WinUIEditor.ModificationFlags.InsertText) == 0) return;
            Equal(callbackText, modification.Text, "later native observers retain a valid counted payload");
            observedCountedPayload = true;
        };
        editor.Modified += retireHandler;
        editor.Modified += inspectHandler;
        editor.SelectAll();
        editor.PasteText(callbackText);
        editor.Modified -= retireHandler;
        editor.Modified -= inspectHandler;
        Check(rejectedUndoRetirement && rejectedMidEditReader && observedCountedPayload,
            "native modification observers cannot retire borrowed undo bytes or freeze an incomplete revision");
        editor.Undo();
        Equal("retire native storage", editor.GetText(editor.Length + 1), "callback protection preserves grouped paste undo");
        GC.KeepAlive(fixture);
    }

    private static async Task CheckNativeJournalContractsAsync()
    {
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync("native-journal-" + Guid.NewGuid().ToString("N"));
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        var restoredFixture = new WinUIEditor.EditorBaseControl();
        var restored = restoredFixture.Editor;
        const string baseline = "ab😀\0tail\r中文";
        var baselineBytes = Encoding.UTF8.GetBytes(baseline);
        WinUIEditor.EditorJournalCheckpoint first = null, latest = null, reopened = null;
        try
        {
            using (var stream = await CreateUtf8StreamAsync(baselineBytes))
            using (var input = stream.GetInputStreamAt(0))
                await editor.LoadUtf8Async(input, (ulong)baselineBytes.Length, false);
            var journal = await folder.CreateFileAsync("active.npj");
            editor.StartJournal(journal.Path, 0);
            var rejectedEolPolicy = false;
            try { editor.EOLMode = WinUIEditor.EndOfLine.CrLf; }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedEolPolicy = true; }
            var rejectedEolConversion = false;
            try { editor.ConvertEOLs(WinUIEditor.EndOfLine.Lf); }
            catch (Exception ex) when (ex.HResult == IllegalMethodCall) { rejectedEolConversion = true; }
            Check(rejectedEolPolicy && rejectedEolConversion && editor.EOLMode == WinUIEditor.EndOfLine.Cr,
                "active journal prevents conflicting native EOL policy and conversion");
            editor.BeginUndoAction();
            editor.SetSel(1, 2);
            editor.PasteText("X\0中");
            editor.EndUndoAction();
            const string edited = "aX\0中😀\0tail\r中文";
            Equal(edited, editor.GetText(editor.Length + 1), "journal fixture has a counted grouped replacement");
            Check(editor.DocumentSequence == 1, "a grouped delete and insert commit one document sequence");
            first = editor.AcquireJournalCheckpoint();
            editor.Undo();
            Check(editor.DocumentSequence == 2, "undo advances the journal sequence independently of savepoint");
            Equal(baseline, editor.GetText(editor.Length + 1), "journal undo restores the baseline");
            editor.Redo();
            Check(editor.DocumentSequence == 3, "redo advances the journal sequence");
            var replacement = Encoding.UTF8.GetBytes("reload😀\0尾");
            using (var stream = await CreateUtf8StreamAsync(replacement))
            using (var input = stream.GetInputStreamAt(0))
                await editor.LoadUtf8Async(input, (ulong)replacement.Length, true);
            Check(editor.DocumentSequence == 4, "an atomic whole-document replacement commits one journal sequence");
            // Reload/Revert journal rotation relies on identical content also committing exactly one step.
            using (var stream = await CreateUtf8StreamAsync(replacement))
            using (var input = stream.GetInputStreamAt(0))
                await editor.LoadUtf8Async(input, (ulong)replacement.Length, true);
            Check(editor.DocumentSequence == 5, "an identical whole-document replacement still commits one journal sequence");
            latest = editor.AcquireJournalCheckpoint();
            await latest.FlushAsync();
            await first.FlushAsync();
            Check(first.CommittedSequence == 1 && first.CommittedDocumentByteLength == (ulong)Encoding.UTF8.GetByteCount(edited),
                "checkpoint captures the exact committed revision and UTF-8 document length");
            Check(first.CommittedByteLength < latest.CommittedByteLength && first.PrefixSha256.Length == 64,
                "earlier committed prefixes remain independently hashable after later edits");
            using (var compacted = new InMemoryRandomAccessStream())
            using (var output = compacted.GetOutputStreamAt(0))
            using (var stream = await CreateUtf8StreamAsync(baselineBytes))
            using (var input = stream.GetInputStreamAt(0))
            {
                await first.WriteBaselineAsync(input, (ulong)baselineBytes.Length, output, 131072);
                var result = new byte[(int)compacted.Size];
                using (var read = compacted.GetInputStreamAt(0))
                    await read.ReadAsync(result.AsBuffer(), (uint)result.Length, InputStreamOptions.None);
                Equal(edited, Encoding.UTF8.GetString(result), "compaction reconstructs a retained prefix while the live editor has later edits");
                Equal("reload😀\0尾", editor.GetText(editor.Length + 1), "background compaction never mutates the live editor");
            }
            using (var rejectedOutput = new InMemoryRandomAccessStream())
            using (var output = rejectedOutput.GetOutputStreamAt(0))
            using (var stream = await CreateUtf8StreamAsync(baselineBytes))
            using (var input = stream.GetInputStreamAt(0))
            {
                var rejectedBudget = false;
                try { await first.WriteBaselineAsync(input, (ulong)baselineBytes.Length, output, (ulong)baselineBytes.Length); }
                catch (ArgumentException) { rejectedBudget = true; }
                Check(rejectedBudget && stream.Position == 0 && rejectedOutput.Size == 0,
                    "native compaction rejects an oversized candidate before reading or writing");
            }
            reopened = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(first.FilePath, first.BaseSequence,
                first.CommittedSequence, first.CommittedByteLength, first.PrefixSha256);
            using (var stream = await CreateUtf8StreamAsync(baselineBytes))
            using (var input = stream.GetInputStreamAt(0))
                await restored.RestoreUtf8Async(input, (ulong)baselineBytes.Length, reopened);
            Equal(edited, restored.GetText(restored.Length + 1), "recovery replays only the requested committed prefix and preserves NUL");
            var invalidPrefix = false;
            try
            {
                using (var invalid = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(latest.FilePath, latest.BaseSequence,
                    latest.CommittedSequence, latest.CommittedByteLength - 1, latest.PrefixSha256)) { }
            }
            catch (Exception) { invalidPrefix = true; }
            Check(invalidPrefix, "recovery rejects a torn committed prefix");
            Equal(edited, restored.GetText(restored.Length + 1), "prefix validation failure leaves the live document intact");
            var continuation = await folder.CreateFileAsync("continued.npj");
            await restored.StartJournalFromCheckpointAsync(continuation.Path, reopened);
            Check(restored.DocumentSequence == 1, "imported journals continue the restored sequence");
            restored.GotoPos(restored.Length);
            var appended = new string('k', 65535) + "😀\0Z";
            restored.PasteText(appended);
            using (var continued = restored.AcquireJournalCheckpoint())
            {
                await continued.FlushAsync();
                Check(continued.CommittedSequence == 2, "chunked INSERT payloads commit one native edit operation");
                using (var verified = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(continued.FilePath, continued.BaseSequence,
                    continued.CommittedSequence, continued.CommittedByteLength, continued.PrefixSha256))
                using (var stream = await CreateUtf8StreamAsync(baselineBytes))
                using (var input = stream.GetInputStreamAt(0))
                {
                    await editor.StopJournalAsync();
                    await editor.RestoreUtf8Async(input, (ulong)baselineBytes.Length, verified);
                }
            }
            Equal(edited + appended, editor.GetText(editor.Length + 1), "imported journal retains its original prefix and split UTF-8 INSERT payload");
            using (var reader = restored.AcquireUtf8Reader())
            {
                Check(reader.Sequence == 2, "reader pins the complete native document sequence");
                var rotated = await folder.CreateFileAsync("rotated.npj");
                await restored.RotateJournalAsync(rotated.Path, reader.Sequence);
                using (var afterRotation = restored.AcquireJournalCheckpoint())
                {
                    await afterRotation.FlushAsync();
                    Check(afterRotation.BaseSequence == 2 && afterRotation.CommittedSequence == 2 && afterRotation.CommittedByteLength == 48,
                        "frozen rotation publishes a fresh journal at the saved revision");
                }
                Check(restored.ReadOnly, "rotation retains the reader's edit freeze");
            }
            Check(!restored.ReadOnly, "rotation restores writability after the final reader closes");
            restored.GotoPos(restored.Length);
            restored.PasteText("!");
            Check(restored.DocumentSequence == 3, "edits continue after save-time journal rotation");
            var nonempty = await folder.CreateFileAsync("nonempty.npj");
            await FileIO.WriteTextAsync(nonempty, "protected");
            var rejectedOverwrite = false;
            try { await restored.RotateJournalAsync(nonempty.Path, restored.DocumentSequence); }
            catch (ArgumentException) { rejectedOverwrite = true; }
            Check(rejectedOverwrite, "native journal creation rejects an existing nonempty destination");
            Equal("protected", await FileIO.ReadTextAsync(nonempty), "failed journal rotation never truncates existing bytes");
            using (var unchanged = restored.AcquireJournalCheckpoint())
                Check(unchanged.BaseSequence == 2 && unchanged.CommittedSequence == 3, "failed rotation retains the prior active writer");
            await CheckContainerJournalContractsAsync(folder);
            await CheckFaultedJournalRecoveryAsync(folder);
            await CheckJournalFlushFormatAndImportAsync(folder);
        }
        finally
        {
            await editor.StopJournalAsync();
            var firstStop = restored.StopJournalAsync();
            var repeatedStop = restored.StopJournalAsync();
            await Task.WhenAll(firstStop.AsTask(), repeatedStop.AsTask());
            first?.Dispose(); latest?.Dispose(); reopened?.Dispose();
            await folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
            GC.KeepAlive(fixture);
            GC.KeepAlive(restoredFixture);
        }
    }

    private static async Task CheckFaultedJournalRecoveryAsync(StorageFolder folder)
    {
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        try
        {
            editor.PasteText("base");
            // The sequence overflow guard is a real writer failure: the next edit latches it.
            var faulted = await folder.CreateFileAsync("faulted.npj");
            editor.StartJournal(faulted.Path, ulong.MaxValue);
            editor.GotoPos(editor.Length);
            editor.PasteText("X");
            Check(editor.JournalFaulted, "a failed journal write is observable");
            var captureFailed = false;
            try { using (editor.AcquireJournalCheckpoint()) { } }
            catch (Exception) { captureFailed = true; }
            Check(captureFailed, "a faulted journal cannot publish a recovery checkpoint");
            using (var reader = editor.AcquireUtf8Reader())
            {
                var rebased = await folder.CreateFileAsync("rebased.npj");
                await editor.RotateJournalAsync(rebased.Path, reader.Sequence);
            }
            Check(!editor.JournalFaulted, "rotating to a fresh baseline clears the journal fault");
            using (var checkpoint = editor.AcquireJournalCheckpoint()) await checkpoint.FlushAsync();
            // The retired writer's earlier failure must not fault shutdown.
            await editor.StopJournalAsync();
        }
        finally
        {
            await editor.StopJournalAsync();
            GC.KeepAlive(fixture);
        }
    }

    private static async Task CheckJournalFlushFormatAndImportAsync(StorageFolder folder)
    {
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        var restoredFixture = new WinUIEditor.EditorBaseControl();
        var restored = restoredFixture.Editor;
        var baseline = Encoding.UTF8.GetBytes("base");
        const string edited = "baseX😀Y";
        try
        {
            editor.PasteText("base");
            var journal = await folder.CreateFileAsync("flush-format.npj");
            editor.StartJournal(journal.Path, 0);
            editor.GotoPos(editor.Length);
            editor.PasteText("X😀");
            ulong committedSequence, committedLength;
            using (var first = editor.AcquireJournalCheckpoint())
            {
                await first.FlushAsync();
                var firstHash = first.PrefixSha256;
                await first.FlushAsync();
                Check(first.PrefixSha256 == firstHash, "flushing an already flushed prefix returns the same verified prefix");
                editor.GotoPos(editor.Length);
                editor.PasteText("Y");
                using (var second = editor.AcquireJournalCheckpoint())
                {
                    await first.FlushAsync();
                    await second.FlushAsync();
                    Check(second.CommittedByteLength > first.CommittedByteLength && first.PrefixSha256 == firstHash,
                        "a longer prefix beyond the flushed range is flushed on its own");
                    using (var onDisk = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(second.FilePath, second.BaseSequence,
                        second.CommittedSequence, second.CommittedByteLength, second.PrefixSha256))
                        Check(onDisk.CommittedDocumentByteLength == second.CommittedDocumentByteLength,
                            "a flushed prefix validates from the file after an earlier skipped flush");
                    committedSequence = second.CommittedSequence;
                    committedLength = second.CommittedByteLength;
                }
            }
            await editor.StopJournalAsync();

            var bytes = (await FileIO.ReadBufferAsync(journal)).ToArray();
            Check((ulong)bytes.Length == committedLength, "the stopped journal holds exactly the committed prefix");
            // Offsets follow the header and frame layouts documented in NativeJournal.cpp.
            var zeroed = BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(40)) == 0;
            // Pre-D7 writers stored CRC32 values in these fields; readers now ignore whatever they hold.
            var legacy = (byte[])bytes.Clone();
            BinaryPrimitives.WriteUInt32LittleEndian(legacy.AsSpan(40), 0xFFFFFFFF);
            var lastPayload = 0;
            for (var offset = 48; offset < legacy.Length;)
            {
                var count = (int)BinaryPrimitives.ReadUInt32LittleEndian(legacy.AsSpan(offset + 16));
                zeroed &= BinaryPrimitives.ReadUInt32LittleEndian(legacy.AsSpan(offset + 48)) == 0;
                BinaryPrimitives.WriteUInt32LittleEndian(legacy.AsSpan(offset + 48), 0xFFFFFFFF);
                if (count != 0) lastPayload = offset + 56;
                offset += 56 + count;
            }
            Check(zeroed, "journal writers store zero in the retired header and frame CRC fields");

            using (var current = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(journal.Path, 0, committedSequence,
                committedLength, Convert.ToHexStringLower(SHA256.HashData(bytes))))
            using (var stream = await CreateUtf8StreamAsync(baseline))
            using (var input = stream.GetInputStreamAt(0))
                await restored.RestoreUtf8Async(input, (ulong)baseline.Length, current);
            Equal(edited, restored.GetText(restored.Length + 1), "a journal with zero CRC fields validates and replays");

            var legacyFile = await folder.CreateFileAsync("legacy-crc.npj");
            await FileIO.WriteBytesAsync(legacyFile, legacy);
            using (var checkpoint = await WinUIEditor.EditorJournalCheckpoint.OpenAsync(legacyFile.Path, 0, committedSequence,
                committedLength, Convert.ToHexStringLower(SHA256.HashData(legacy))))
            {
                using (var stream = await CreateUtf8StreamAsync(baseline))
                using (var input = stream.GetInputStreamAt(0))
                    await restored.RestoreUtf8Async(input, (ulong)baseline.Length, checkpoint);
                Equal(edited, restored.GetText(restored.Length + 1), "a legacy journal with nonzero CRC fields validates and replays");

                // Change validated bytes without breaking structure: only the prefix hash can catch it.
                legacy[lastPayload] ^= 0x01;
                await FileIO.WriteBytesAsync(legacyFile, legacy);
                var candidate = await folder.CreateFileAsync("rejected-import.npj");
                var rejectedImport = false;
                try { await restored.StartJournalFromCheckpointAsync(candidate.Path, checkpoint); }
                catch (Exception) { rejectedImport = true; }
                var adopted = true;
                try { using (restored.AcquireJournalCheckpoint()) { } }
                catch (Exception) { adopted = false; }
                Check(rejectedImport && !adopted, "journal import rejects a prefix whose bytes no longer match its hash");
                Equal(edited, restored.GetText(restored.Length + 1), "a rejected import leaves the document unchanged");
            }
        }
        finally
        {
            await editor.StopJournalAsync();
            await restored.StopJournalAsync();
            GC.KeepAlive(fixture);
            GC.KeepAlive(restoredFixture);
        }
    }

    private static async Task CheckContainerJournalContractsAsync(StorageFolder folder)
    {
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        var restoredFixture = new WinUIEditor.EditorBaseControl();
        var restored = restoredFixture.Editor;
        var canceledFixture = new WinUIEditor.EditorBaseControl();
        var canceled = canceledFixture.Editor;
        var baseline = Encoding.UTF8.GetBytes("base");
        try
        {
            editor.PasteText("base");
            var journal = await folder.CreateFileAsync("container-boundary.npj");
            editor.StartJournal(journal.Path, 0);
            editor.BeginUndoAction();
            editor.AddUndoAction(101, WinUIEditor.UndoFlags.None);
            editor.GotoPos(editor.Length);
            editor.PasteText("X");
            editor.EndUndoAction();
            editor.Undo();
            Check(editor.DocumentSequence == 2, "a container action ending undo completes its text journal operation");
            using (var undo = editor.AcquireJournalCheckpoint()) await undo.FlushAsync();
            Equal("base", editor.GetText(editor.Length + 1), "container-ended undo restores the original text");
            editor.Redo();
            editor.BeginUndoAction();
            editor.GotoPos(editor.Length);
            editor.PasteText("Y");
            editor.AddUndoAction(102, WinUIEditor.UndoFlags.None);
            editor.EndUndoAction();
            editor.Undo();
            editor.Redo();
            Check(editor.DocumentSequence == 6, "a container action ending redo completes its text journal operation");
            using (var checkpoint = editor.AcquireJournalCheckpoint())
            {
                await checkpoint.FlushAsync();
                using (var stream = await CreateUtf8StreamAsync(baseline))
                using (var input = stream.GetInputStreamAt(0))
                    await restored.RestoreUtf8Async(input, (ulong)baseline.Length, checkpoint);
                Equal("baseXY", restored.GetText(restored.Length + 1), "container-ended operations replay exactly into native recovery");
                using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("baseXY")))
                using (var input = stream.GetInputStreamAt(0))
                    await canceled.LoadUtf8Async(input, 6, false);
                var candidate = await folder.CreateFileAsync("canceled-import.npj");
                var import = canceled.StartJournalFromCheckpointAsync(candidate.Path, checkpoint);
                Check(!canceled.ReadOnly, "immutable journal import preparation keeps editing enabled");
                import.Cancel();
                try { await import; }
                catch (OperationCanceledException) { }
                Check(!canceled.ReadOnly, "canceling journal preparation leaves no private edit freeze");
                using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes("after canceled import")))
                using (var input = stream.GetInputStreamAt(0))
                    await canceled.LoadUtf8Async(input, stream.Size, false);
                Equal("after canceled import", canceled.GetText(canceled.Length + 1),
                    "a replacement can immediately follow canceled journal preparation");
            }
        }
        finally
        {
            await editor.StopJournalAsync();
            await restored.StopJournalAsync();
            await canceled.StopJournalAsync();
            GC.KeepAlive(fixture);
            GC.KeepAlive(restoredFixture);
            GC.KeepAlive(canceledFixture);
        }
    }

    private static async Task CheckNativeFinalizerReleaseAsync()
    {
        WriteDiagnostic("Native lifecycle: forcing finalizer-thread release");
        var fixtures = CreateNativeFinalizerFixtures();
        // Leave UI free while managed finalizers release their native objects.
        await Task.Run(() => { GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect(); });
        await Task.Delay(100);
        Check(!fixtures[0].IsAlive && !fixtures[1].IsAlive,
            "native editor wrappers can be collected while owning UI remains active");
        WriteDiagnostic("Native lifecycle: finalizer-thread release completed");
    }

    private static async Task CheckDisposeReleasesDocumentAsync()
    {
        var closed = new TextEditorCore();
        await closed.LoadTextAsync("closed tab😀\r");
        var native = ((WinUIEditor.EditorBaseControl)((Grid)closed.Content).Children[0]).Editor;
        Check(native.Length != 0, "the disposal fixture has text");
        closed.Dispose();
        Check(native.Length == 0 && closed.GetText().Length == 0, "Dispose releases the native document");
        closed.Dispose();

        var leased = new TextEditorCore();
        await leased.LoadTextAsync("leased tab\r");
        var leasedNative = ((WinUIEditor.EditorBaseControl)((Grid)leased.Content).Children[0]).Editor;
        using (leasedNative.AcquireUtf8Reader()) leased.Dispose();
        Check(leasedNative.Length != 0, "Dispose keeps a document that a reader still leases");
    }

    private static void Equal(string expected, string actual, string test)
    {
        Check(expected == actual,
            test + $" (expected {expected.Length}: {EscapeText(expected)}, got {actual.Length}: {EscapeText(actual)})");
    }

    private static string EscapeText(string text)
    {
        var escaped = text.Replace("\\", "\\\\").Replace("\0", "\\0").Replace("\r", "\\r")
            .Replace("\n", "\\n").Replace("\t", "\\t");
        return escaped.Length <= 160 ? escaped : escaped.Substring(0, 160) + "...";
    }

    private static void CheckSelectionPalette(WinUIEditor.Editor native, uint background)
    {
        foreach (var element in new[] { WinUIEditor.Element.SelectionBack, WinUIEditor.Element.SelectionAdditionalBack,
            WinUIEditor.Element.SelectionSecondaryBack, WinUIEditor.Element.SelectionInactiveBack,
            WinUIEditor.Element.SelectionInactiveAdditionalBack })
        {
            Check(unchecked((uint)native.GetElementColour(element)) == background, element + " uses accent for every selection range");
        }

        foreach (var element in new[] { WinUIEditor.Element.SelectionText, WinUIEditor.Element.SelectionAdditionalText,
            WinUIEditor.Element.SelectionSecondaryText, WinUIEditor.Element.SelectionInactiveText,
            WinUIEditor.Element.SelectionInactiveAdditionalText })
        {
            Check(unchecked((uint)native.GetElementColour(element)) == 0xffffffff, element + " uses white selected text");
        }
    }

    // EditorThemes.xaml keeps the editor colors that were previously hard-coded.
    private static void CheckEditorPalette()
    {
        var light = EditorColorPalette.ForTheme(ElementTheme.Light, highContrast: false);
        var dark = EditorColorPalette.ForTheme(ElementTheme.Dark, highContrast: false);
        Check(light[EditorColorRole.Background] == Windows.UI.Colors.Transparent &&
            dark[EditorColorRole.Background] == Windows.UI.Colors.Transparent, "editor background stays transparent");
        Check(light[EditorColorRole.Foreground] == Windows.UI.Colors.Black &&
            dark[EditorColorRole.Foreground] == Windows.UI.Colors.White, "editor foreground resources");
        Check(light[EditorColorRole.LineNumber] == Windows.UI.Color.FromArgb(255, 105, 105, 105) &&
            dark[EditorColorRole.LineNumber] == Windows.UI.Color.FromArgb(255, 150, 150, 150), "line number resources");
        Check(light[EditorColorRole.CaretLine] == Windows.UI.Color.FromArgb(24, 0, 0, 0) &&
            dark[EditorColorRole.CaretLine] == Windows.UI.Color.FromArgb(24, 255, 255, 255), "line highlight resources");
        Check(light[EditorColorRole.SelectionText] == Windows.UI.Colors.White &&
            dark[EditorColorRole.SelectionText] == Windows.UI.Colors.White, "selected text resources");
    }

    // Restyling re-wraps a wrapped document. A tab switch, a repeated theme
    // notification or an unchanged font size must keep the applied styles.
    private static async Task CheckThemeAppliesOnlyOnChangeAsync(Panel host, TextEditorCore core, WinUIEditor.Editor native)
    {
        const int sentinel = 0x123456;
        native.StyleSetFore(LineNumberStyle, sentinel);
        var loaded = new TaskCompletionSource();
        void OnReloaded(object sender, RoutedEventArgs args) => loaded.TrySetResult();
        core.Loaded += OnReloaded;
        host.Children.Remove(core);
        await Task.Delay(50);
        host.Children.Add(core);
        await Task.WhenAny(loaded.Task, Task.Delay(2000));
        core.Loaded -= OnReloaded;
        Check(loaded.Task.IsCompleted && native.StyleGetFore(LineNumberStyle) == sentinel, "re-entering the visual tree keeps the applied styles");
        var requestedTheme = core.RequestedTheme;
        core.RequestedTheme = ThemeSettingsService.ThemeMode == ElementTheme.Dark ? ElementTheme.Light : ElementTheme.Dark;
        await Task.Delay(50);
        core.RequestedTheme = requestedTheme;
        await Task.Delay(50);
        Check(native.StyleGetFore(LineNumberStyle) == sentinel, "an unchanged app theme skips ActualThemeChanged restyling");
        core.SetFontZoomFactor(core.GetFontZoomFactor());
        Check(native.StyleGetFore(LineNumberStyle) == sentinel, "an unchanged font size skips restyling");
        core.SetFontZoomFactor(core.GetFontZoomFactor() + 10);
        Check(native.StyleGetFore(LineNumberStyle) != sentinel, "a font size change restyles");
        core.SetFontZoomFactor(core.GetFontZoomFactor() - 10);
    }

    private static async Task CheckLoadedIndentationAsync(WinUIEditor.Editor editor)
    {
        var tabWidth = editor.TabWidth;
        var indent = editor.Indent;
        var useTabs = editor.UseTabs;
        var tabIndents = editor.TabIndents;
        var backspaceUnindents = editor.BackSpaceUnIndents;
        try
        {
            editor.TabWidth = 4;
            editor.UseTabs = false;
            editor.TabIndents = true;
            editor.BackSpaceUnIndents = true;
            foreach (var size in new[] { 0, 2 })
            {
                editor.Indent = size;
                using var stream = new MemoryStream(Encoding.UTF8.GetBytes("    x"));
                using var input = stream.AsInputStream();
                await editor.LoadUtf8Async(input, (ulong)stream.Length, false);
                editor.SetSel(4, 4);
                editor.DeleteBack();
                Equal(size == 0 ? "x" : "  x", editor.GetText(editor.Length + 1),
                    "loaded documents retain actual backspace indentation with implicit and explicit indent size");
            }
        }
        finally
        {
            editor.TabWidth = tabWidth;
            editor.Indent = indent;
            editor.UseTabs = useTabs;
            editor.TabIndents = tabIndents;
            editor.BackSpaceUnIndents = backspaceUnindents;
        }
    }

    private static void CheckNativeEditingCommands(TextEditorCore core, WinUIEditor.Editor native)
    {
        WriteDiagnostic("Native editing: starting indentation checks");
        var previousIndent = ApplicationPreferences.EditorDefaultTabIndents;
        try
        {
            foreach (var indent in new[] { -1, 2, 4, 8 })
            {
                WriteDiagnostic("Native editing: indentation fixture " + indent);
                ApplicationPreferences.EditorDefaultTabIndents = indent;
                var prefix = indent < 0 ? "\t" : new string(' ', indent);
                core.SetText("ab");
                core.SetTextSelectionPosition(1, 1);
                core.TestIndentation(false);
                WriteDiagnostic("Native editing: inserted single Tab " + indent);
                Equal("a" + prefix + "b", core.GetText(), "single Tab inserts the configured fixed text");
                Check(native.TabWidth == 4, "Tab preference does not alter display of existing tabs");

                core.SetText("aa\rbb\rcc");
                core.SetTextSelectionPosition(6, 0);
                core.TestIndentation(false);
                Equal(prefix + "aa\r" + prefix + "bb\rcc", core.GetText(), "native indentation excludes an unselected final line");
                Check(native.CurrentPos < native.Anchor, "native indentation preserves reversed selection direction");
                core.TestIndentation(true);
                WriteDiagnostic("Native editing: dedented selected lines " + indent);
                Equal("aa\rbb\rcc", core.GetText(), "native dedentation restores selected lines");
                core.Undo();
                Equal(prefix + "aa\r" + prefix + "bb\rcc", core.GetText(), "dedentation is one undo action");
                core.Undo();
                Equal("aa\rbb\rcc", core.GetText(), "indentation is one undo action");

                core.SetText(prefix + prefix + "aa");
                core.SetTextSelectionPosition(core.GetText().Length, core.GetText().Length);
                core.TestIndentation(true);
                Equal(prefix + "aa", core.GetText(), "single Shift+Tab dedents even after the text");
                core.Undo();
                Equal(prefix + prefix + "aa", core.GetText(), "single dedentation undo");
            }
            ApplicationPreferences.EditorDefaultTabIndents = 4;
            WriteDiagnostic("Native editing: rectangle Tab fixture");
            core.SetText("aa\rbb\rcc");
            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 1;
            native.RectangularSelectionCaret = 8;
            Check(native.Selections == 3, "three-row Tab fixture");
            core.TestIndentation(false);
            Equal("a   \rb   \rc   ", core.GetText(), "native Tab edits each rectangular range independently");
            core.Undo();
            Equal("aa\rbb\rcc", core.GetText(), "rectangular Tab is one undo action");
            native.SelectionMode = WinUIEditor.SelectionMode.Stream;

            WriteDiagnostic("Native editing: rectangle selection/deletion fixture");
            core.SetText("aa\rbb\rcc");
            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 1;
            native.RectangularSelectionCaret = 8;
            core.GetLineColumnSelection(out _, out _, out _, out _, out var rectangleSelected, out _);
            Check(rectangleSelected == 3, "rectangular selected count includes only the selected glyphs");
            core.SmartlyTrimTextSelection();
            Check(native.SelectionIsRectangle && native.Selections == 3,
                "smart copy preserves rectangular selection ranges");
            native.RectangularSelectionCaret = 7;
            core.GetLineColumnSelection(out _, out _, out _, out _, out rectangleSelected, out _);
            Check(!core.HasSelection && rectangleSelected == 0,
                "zero-width rectangle has no selected characters");
            native.RectangularSelectionCaret = 8;
            core.DeleteSelection();
            Equal("a\rb\rc", core.GetText(), "rectangular deletion preserves unselected text between rows");
            core.Undo();
            Equal("aa\rbb\rcc", core.GetText(), "rectangular deletion is one undo action");

            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 1;
            native.RectangularSelectionCaret = 8;
            core.TypeText("😀\0尾");
            WriteDiagnostic("Native editing: pasted rectangle Unicode/NUL text");
            Equal("a😀\0尾\rb😀\0尾\rc😀\0尾", core.GetText(), "counted native paste replaces every rectangular range");
            core.Undo();
            Equal("aa\rbb\rcc", core.GetText(), "rectangular Unicode and NUL paste is one undo action");

            native.SelectionMode = WinUIEditor.SelectionMode.Rectangle;
            native.RectangularSelectionAnchor = 1;
            native.RectangularSelectionCaret = 8;
            core.TypeText("x\r\ny");
            WriteDiagnostic("Native editing: pasted rectangle multiline text");
            Equal("ax\ry\rbx\ry\rcx\ry", core.GetText(), "rectangular paste normalizes line endings in each range");
            core.Undo();
            Equal("aa\rbb\rcc", core.GetText(), "rectangular multiline paste is one undo action");
            native.SelectionMode = WinUIEditor.SelectionMode.Stream;

            var previousMultipleSelection = native.MultipleSelection;
            native.MultipleSelection = true;
            core.SetText("aaa bbb ccc");
            native.SetSelection(2, 1);
            native.AddSelection(10, 9);
            core.TypeText("X\0");
            Equal("aX\0a bbb cX\0c", core.GetText(), "native paste keeps disjoint selection ranges separate");
            core.Undo();
            Equal("aaa bbb ccc", core.GetText(), "multiple-range paste is one undo action");
            native.MultipleSelection = previousMultipleSelection;

            core.SetText("中文\r😀\rtail\runselected");
            core.SetTextSelectionPosition(0, 10);
            core.TestJoinLines();
            Equal("中文 😀 tail\runselected", core.GetText(), "native line join keeps Unicode and unselected lines");
            Equal("中文 😀 tail", core.GetSelectedText(), "line join selects the resulting native target");
            core.Undo();
            Equal("中文\r😀\rtail\runselected", core.GetText(), "line join is one undo action");
            core.SetText("one \rtwo\rthree");
            core.SelectAll();
            core.TestJoinLines();
            Equal("one two three", core.GetText(), "native join reuses an existing separating space");
            core.SetText("one\rtwo\rthree");
            core.SetTextSelectionPosition(0, 4);
            core.TestJoinLines();
            Equal("one\rtwo\rthree", core.GetText(), "join excludes an unselected final line");
        }
        finally { ApplicationPreferences.EditorDefaultTabIndents = previousIndent; }
    }

    private static async Task CheckSearchBehaviorAsync(TextEditorCore core)
    {
        WriteDiagnostic("Search behavior: preparing case-sensitive text");
        var caseSensitive = new SearchContext("TEST", matchCase: true);
        core.SetText("test TEST tester test_value");
        core.SetTextSelectionPosition(0, 0);
        WriteDiagnostic("Search behavior: case-sensitive forward native search");
        Check(((await core.FindAsync(caseSensitive, false, true)).Status == WinUIEditor.EditorSearchStatus.Found), "case-sensitive next search");
        Equal("TEST", core.GetSelectedText(), "case-sensitive selection");
        Check(!((await core.FindAsync(caseSensitive, false, true)).Status == WinUIEditor.EditorSearchStatus.Found), "next search stops at EOF");
        Check(((await core.FindAsync(caseSensitive, false, false)).Status == WinUIEditor.EditorSearchStatus.Found), "next search wraps at EOF");
        core.SetTextSelectionPosition(0, 0);
        Check(((await core.FindAsync(caseSensitive, true, false)).Status == WinUIEditor.EditorSearchStatus.Found), "previous search wraps at BOF");
        Equal("TEST", core.GetSelectedText(), "case-sensitive previous selection");

        WriteDiagnostic("Search behavior: whole-word native search");
        core.SetText("test_value test");
        core.SetTextSelectionPosition(0, 0);
        var wholeWord = new SearchContext("test", matchWholeWord: true);
        Check(((await core.FindAsync(wholeWord, false, false)).Status == WinUIEditor.EditorSearchStatus.Found), "whole-word next search");
        core.GetTextSelectionPosition(out var start, out var end);
        Check(start == 11 && end == 15, "Scintilla whole-word search treats underscore as part of a word");
        foreach (var previous in new[] { false, true })
        {
            core.SetText("a needle b");
            core.SetTextSelectionPosition(5, 5);
            Check((await core.FindAsync(new SearchContext("needle"), previous, false)).Status == WinUIEditor.EditorSearchStatus.Found,
                $"wrapped search finds the sole match around the caret (previous: {previous})");
            core.GetTextSelectionPosition(out start, out end);
            Check(start == 2 && end == 8, $"wrapped search selects the sole match around the caret (previous: {previous})");
        }
        core.SetText("test TEST tester test_value");
        Check(((await core.ReplaceAllAsync(new SearchContext("test", matchCase: true, matchWholeWord: true), "X")).Status == WinUIEditor.EditorSearchStatus.Found), "combined case-sensitive whole-word replacement");
        Equal("X TEST tester test_value", core.GetText(), "case-sensitive whole-word replacement result");

        WriteDiagnostic("Search behavior: blocked mutation during native reads");
        var literal = new SearchContext("cat");
        var native = ((WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
        core.SetText("cat cat");
        core.SetTextSelectionPosition(0, 3);
        var contentVersion = core.ContentVersion;
        using (native.AcquireUtf8Reader())
        {
            Check(!((await core.ReplaceAllAsync(literal, string.Empty)).Status == WinUIEditor.EditorSearchStatus.Found),
                "read lease rejects literal deletion without looping");
            Check(!((await core.ReplaceAllAsync(new SearchContext("cat", useRegex: true), "dog")).Status == WinUIEditor.EditorSearchStatus.Found),
                "read lease rejects regex replacements");
            Check(!((await core.ReplaceAsync(literal, string.Empty, false)).Status == WinUIEditor.EditorSearchStatus.Found) &&
                !((await core.ReplaceAsync(literal, "dog", true)).Status == WinUIEditor.EditorSearchStatus.Found), "read lease rejects single replacements");
            core.TypeText("blocked");
            core.DeleteSelection();
            core.Undo();
            core.Redo();
            Check(core.ContentVersion == contentVersion, "read lease keeps command mutation out of the document");
            Equal("cat cat", core.GetText(), "read lease leaves every byte unchanged");
            core.GetTextSelectionPosition(out start, out end);
            Check(start == 0 && end == 3, "rejected replacement preserves the selected match");
        }
        core.IsEnabled = false;
        try
        {
            Check(!((await core.ReplaceAllAsync(literal, string.Empty)).Status == WinUIEditor.EditorSearchStatus.Found) &&
                !((await core.ReplaceAllAsync(new SearchContext("cat", useRegex: true), "dog")).Status == WinUIEditor.EditorSearchStatus.Found),
                "disabled editor rejects literal and regex Replace All");
            Check(!((await core.ReplaceAsync(literal, "dog", false)).Status == WinUIEditor.EditorSearchStatus.Found) &&
                !((await core.ReplaceAsync(literal, string.Empty, true)).Status == WinUIEditor.EditorSearchStatus.Found), "disabled editor rejects single replacements");
            Check(core.ContentVersion == contentVersion, "disabled replacement leaves the native generation unchanged");
            Equal("cat cat", core.GetText(), "disabled replacement leaves every byte unchanged");
        }
        finally { core.IsEnabled = true; }
        Check(((await core.ReplaceAllAsync(literal, string.Empty)).Status == WinUIEditor.EditorSearchStatus.Found), "writable empty replacement deletes every match");
        Equal(" ", core.GetText(), "empty replacement result");
        core.Undo();
        Equal("cat cat", core.GetText(), "empty Replace All retains a single undo boundary");

        WriteDiagnostic("Search behavior: single literal replacements");
        core.SetText("cat cat");
        core.SetTextSelectionPosition(0, 3);
        Check(((await core.ReplaceAsync(literal, "dog", false)).Status == WinUIEditor.EditorSearchStatus.Found), "single forward replacement");
        Equal("dog cat", core.GetText(), "single replacement edits selected match");
        Equal("cat", core.GetSelectedText(), "single replacement selects next match");
        core.SetText("cat cat");
        core.SetTextSelectionPosition(4, 7);
        Check(((await core.ReplaceAsync(literal, "dog", true)).Status == WinUIEditor.EditorSearchStatus.Found), "single backward replacement");
        Equal("cat dog", core.GetText(), "backward replacement edits selected match");
        Equal("cat", core.GetSelectedText(), "backward replacement selects previous match");

        WriteDiagnostic("Search behavior: regex selection and replacements");
        var regex = new SearchContext("^a(\\d)$", useRegex: true);
        core.SetText("a1\ra2");
        core.SetTextSelectionPosition(0, 0);
        Check(((await core.FindAsync(regex, false, false)).Status == WinUIEditor.EditorSearchStatus.Found), "multiline regex next search");
        Equal("a1", core.GetSelectedText(), "regex next selection");
        core.SetTextSelectionPosition(5, 5);
        Check(((await core.FindAsync(regex, true, false)).Status == WinUIEditor.EditorSearchStatus.Found), "multiline regex previous search");
        Equal("a2", core.GetSelectedText(), "regex previous selection");
        core.SetTextSelectionPosition(0, 0);
        Check(((await core.ReplaceAsync(regex, "$1", false)).Status == WinUIEditor.EditorSearchStatus.Found), "single regex replacement");
        Equal("$1\ra2", core.GetText(), "v1 single regex replacement preserves literal capture syntax");
        core.SetText("x a1 y\ra2 tail");
        core.SetTextSelectionPosition(0, 0);
        Check(((await core.ReplaceAsync(new SearchContext("a(\\d)", useRegex: true), "b", false)).Status == WinUIEditor.EditorSearchStatus.Found),
            "single regex replacement before undo");
        Equal("x b y\ra2 tail", core.GetText(), "single regex replacement edits only its match");
        core.Undo();
        Equal("x a1 y\ra2 tail", core.GetText(), "undo restores a single regex replacement");
        Check(native.CurrentPos <= 4, "undoing a single regex replacement keeps the caret at the match, not the document end");
        core.SetText("a1 x a2");
        core.SetTextSelectionPosition(7, 7);
        Check(((await core.ReplaceAsync(new SearchContext("a(\\d)", useRegex: true), "b", true)).Status == WinUIEditor.EditorSearchStatus.Found),
            "single backward regex replacement");
        Equal("a1 x b", core.GetText(), "single backward regex replacement edits only the previous match");
        core.Undo();
        Equal("a1 x a2", core.GetText(), "undo restores a single backward regex replacement");
        Check(native.CurrentPos >= 5, "undoing a single backward regex replacement keeps the caret at the match, not the document start");
        core.SetText("a1\ra2");
        Check(((await core.ReplaceAllAsync(regex, "$1\\tX\\n")).Status == WinUIEditor.EditorSearchStatus.Found), "regex replacement escapes");
        Equal("1\tX\r\r2\tX\r", core.GetText(), "regex capture and tab/newline expansion");
        WriteDiagnostic("Search behavior: invalid regex reporting");
        core.SetText("unchanged");
        var invalidRegex = new SearchContext("[", useRegex: true);
        Check((await core.FindAsync(invalidRegex)).Status == WinUIEditor.EditorSearchStatus.InvalidPattern, "invalid regex search reports error");
        Check((await core.ReplaceAllAsync(invalidRegex, "X")).Status == WinUIEditor.EditorSearchStatus.InvalidPattern, "invalid regex replacement reports error");
        Equal("unchanged", core.GetText(), "invalid regex leaves document unchanged");

        WriteDiagnostic("Search behavior: bounded Find seed");
        var longWord = new string('x', (int)TextEditorCore.SearchPatternLimit + 1);
        core.SetText("seed " + longWord);
        core.SetTextSelectionPosition(0, 4);
        Equal("seed", core.GetSearchString(), "a normal selection seeds Find");
        core.SetTextSelectionPosition(0, 5 + longWord.Length);
        Equal(string.Empty, core.GetSearchString(), "a selection over the pattern cap does not seed Find");
        core.SetTextSelectionPosition(2, 2);
        Equal("seed", core.GetSearchString(), "the caret word seeds Find");
        core.SetTextSelectionPosition(10, 10);
        Equal(string.Empty, core.GetSearchString(), "a caret word over the pattern cap does not seed Find");
        await CheckNativeRegexBehaviorAsync(core);
    }

    private static async Task CheckNativeRegexBehaviorAsync(TextEditorCore core)
    {
        WriteDiagnostic("Native regex: UTF-8 gaps, Unicode, multiline and substitutions");
        var fixture = new WinUIEditor.EditorBaseControl();
        var editor = fixture.Editor;
        editor.EOLMode = WinUIEditor.EndOfLine.Cr;
        const string counted = "start😀\0\rعربي中文end";
        editor.SetText(string.Empty);
        editor.AddText(Encoding.UTF8.GetByteCount(counted), counted);
        var found = await editor.FindRegexAsync("😀.\\n(?=عربي)", true, 0, false, false, -1);
        Check(found.Status == WinUIEditor.EditorSearchStatus.Found && found.Start == 5 && found.End == 11,
            "native regex uses Unicode scalars and counted byte offsets across NUL and CR");
        editor.SetText("a\rb");
        var newline = await editor.ReplaceRegexAsync("(a\\n)", true, "$1\\n", 0, false, true);
        Check(newline.Status == WinUIEditor.EditorSearchStatus.Found, "native regex newline replacement succeeds");
        Equal("a\r\rb", editor.GetText(editor.Length + 1), "captured logical LF does not collapse a following replacement LF");
        editor.Undo();
        Equal("a\rb", editor.GetText(editor.Length + 1), "native regex replacement is one undo action");

        editor.SetText("ab");
        var groups = await editor.ReplaceRegexAsync("(?<x>a)(b)", true, "$1-${x}-$2-$+-$10-${missing}", 0, false, true);
        Check(groups.Status == WinUIEditor.EditorSearchStatus.Found, "mixed native capture replacement succeeds");
        Equal("a-a-b-b-$10-${missing}", editor.GetText(editor.Length + 1), "ICU numbering and unknown reference literals are explicit");
        editor.SetText("abc");
        await editor.ReplaceRegexAsync("(b)", true, "$`|$'|$_|$&|$$|$0|${0}", 0, false, true);
        Equal("aa|c|abc|b|$|b|bc", editor.GetText(editor.Length + 1), "prefix suffix entire input and complete match tokens");
        editor.SetText("y");
        await editor.ReplaceRegexAsync("(x)?y", true, "$1-${1}-${missing}-${-$", 0, false, true);
        Equal("--${missing}-${-$", editor.GetText(editor.Length + 1), "unmatched captures empty and malformed tokens literal");
        editor.SetText("a1 a2");
        await editor.ReplaceRegexAsync("a(\\d)", true, "X", 0, true, true);
        Equal("X X", editor.GetText(editor.Length + 1), "native Replace All ignores navigation direction and origin");
        editor.SetText("b");
        await editor.ReplaceRegexAsync("(a)?b", true, "$+", 0, false, true);
        Equal(string.Empty, editor.GetText(editor.Length + 1), "last participating capture is empty when no declared capture participates");
        core.SetText("aa");
        core.SetTextSelectionPosition(0, 1);
        Check((await core.ReplaceAsync(new SearchContext("a", useRegex: true), string.Empty)).Status == WinUIEditor.EditorSearchStatus.Found,
            "single regex deletion commits");
        Equal("a", core.GetText(), "single regex deletion preserves remaining text");
        Equal("a", core.GetSelectedText(), "single regex deletion finds the next match at the replacement start");

        core.SetText("😀\rx");
        core.SetTextSelectionPosition(0, 0);
        var empty = new SearchContext("^|$", useRegex: true);
        foreach (var position in new[] { 0, 2, 3, 4, 0 })
        {
            Check((await core.FindAsync(empty)).Status == WinUIEditor.EditorSearchStatus.Found, "zero-width next progresses and wraps");
            core.GetTextSelectionPosition(out var start, out var end);
            Check(start == position && end == position, "zero-width navigation uses scalar boundaries");
        }
        core.SetText("");
        core.SetTextSelectionPosition(0, 0);
        Check((await core.FindAsync(empty)).Status == WinUIEditor.EditorSearchStatus.Found, "first empty-document match at caret");
        Check((await core.FindAsync(empty)).Status == WinUIEditor.EditorSearchStatus.NotFound, "wrap excludes the same sole empty match");
        core.SetTextSelectionPosition(0, 0);
        Check((await core.FindAsync(empty)).Status == WinUIEditor.EditorSearchStatus.Found, "explicit navigation resets empty-match identity");
        core.SetText("aaaa");
        core.SetTextSelectionPosition(2, 2);
        Check((await core.FindAsync(new SearchContext("a+", useRegex: true), true, true)).Status == WinUIEditor.EditorSearchStatus.NotFound,
            "previous search excludes a full-input match spanning the origin");

        editor.SetText("unchanged");
        var limited = await editor.FindRegexAsync(new string('a', (int)TextEditorCore.SearchPatternLimit + 1), true, 0, false, false, -1);
        Check(limited.Status == WinUIEditor.EditorSearchStatus.ResourceLimit, "bounded pattern reports a distinct resource limit");
        editor.SetText(new string('a', 4096) + "!");
        var timeout = await editor.FindRegexAsync("(a+)+$", true, 0, false, false, -1);
        Check(timeout.Status == WinUIEditor.EditorSearchStatus.TimedOut || timeout.Status == WinUIEditor.EditorSearchStatus.ResourceLimit,
            "adversarial regex has a typed execution or stack budget");
        editor.EmptyUndoBuffer();
        var canceled = editor.FindRegexAsync("(a+)+$", true, 0, false, false, -1);
        await Task.Delay(10);
        Check(editor.ReadOnly, "native search holds its stable-buffer lease while the UI remains responsive");
        editor.SelectAll();
        editor.UpperCase();
        editor.Clear();
        Check(!editor.CanUndo(), "internal undo groups are frozen with leased text");
        canceled.Cancel();
        try { await canceled; }
        catch (OperationCanceledException) { }
        Check(!editor.ReadOnly, "canceled regex unwinds its native read lease before completion");
        using (var cancellation = new System.Threading.CancellationTokenSource())
        {
            await core.LoadTextAsync(new string('a', 4096) + "!");
            var pending = core.FindAsync(new SearchContext("(a+)+$", useRegex: true),
                cancellationToken: cancellation.Token);
            cancellation.Cancel();
            try { await pending; }
            catch (OperationCanceledException) { }
            Check(!((WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0]).Editor.ReadOnly,
                "managed cancellation awaits native cleanup before releasing admission");
        }
        using (var cancellation = new System.Threading.CancellationTokenSource())
        {
            var retiringCore = new TextEditorCore();
            await retiringCore.LoadTextAsync(new string('a', 4096) + "!");
            var retiringEditor = ((WinUIEditor.EditorBaseControl)((Grid)retiringCore.Content).Children[0]).Editor;
            var retiringSearch = retiringCore.FindAsync(new SearchContext("(a+)+$", useRegex: true),
                cancellationToken: cancellation.Token);
            retiringCore.Dispose();
            cancellation.Cancel();
            try { await retiringSearch; }
            catch (OperationCanceledException) { }
            Check(!retiringEditor.ReadOnly, "disposing the managed adapter during search retains and drains its native owner");
        }

        WriteDiagnostic("Native regex: atomic publication rejection and staged recovery journal");
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync("regex-journal-" + Guid.NewGuid().ToString("N"));
        var file = await folder.CreateFileAsync("active.npj");
        const string baseline = "cat cat\r😀";
        editor.SetText(baseline);
        editor.EmptyUndoBuffer();
        editor.SetSavePoint();
        editor.StartJournal(file.Path, 0);
        try
        {
            var rejectedOperation = editor.ReplaceRegexAsync("cat", true, "dog", 0, false, true);
            editor.SetSel(1, 1);
            var rejected = await rejectedOperation;
            Check(rejected.Status == WinUIEditor.EditorSearchStatus.Stale, "navigation rejects stale prepared publication before commit");
            Equal(baseline, editor.GetText(editor.Length + 1), "rejected publication leaves every byte unchanged");
            Check(editor.DocumentSequence == 0 && !editor.CanUndo() && !editor.ReadOnly,
                "rejected publication restores undo savepoint journal sequence and mutation admission");
            var attemptedReentrantEdit = false;
            Windows.Foundation.IAsyncOperation<WinUIEditor.EditorSearchResult> committing = null;
            WinUIEditor.ModifiedHandler reentrant = (sender, args) =>
            {
                if ((args.ModificationType & (int)(WinUIEditor.ModificationFlags.BeforeDelete | WinUIEditor.ModificationFlags.InsertText)) == 0) return;
                attemptedReentrantEdit = true;
                sender.PasteText("must not enter a journal preparation");
                if ((args.ModificationType & (int)WinUIEditor.ModificationFlags.InsertText) != 0) committing.Cancel();
            };
            editor.Modified += reentrant;
            committing = editor.ReplaceRegexAsync("cat", true, "dog", 0, false, true);
            var committed = await committing;
            editor.Modified -= reentrant;
            Check(attemptedReentrantEdit && committed.Status == WinUIEditor.EditorSearchStatus.Found && editor.DocumentSequence == 1,
                "late cancellation preserves committed replacement and exactly one journal operation");
            Equal("dog dog\r😀", editor.GetText(editor.Length + 1), "reentrant edits remain blocked through journal Commit");
            using (var checkpoint = editor.AcquireJournalCheckpoint())
            {
                await checkpoint.FlushAsync();
                var recoveredFixture = new WinUIEditor.EditorBaseControl();
                var recovered = recoveredFixture.Editor;
                using (var stream = await CreateUtf8StreamAsync(Encoding.UTF8.GetBytes(baseline)))
                using (var input = stream.GetInputStreamAt(0))
                    await recovered.RestoreUtf8Async(input, (ulong)Encoding.UTF8.GetByteCount(baseline), checkpoint);
                Equal("dog dog\r😀", recovered.GetText(recovered.Length + 1), "Abort then Commit replay reconstructs the exact replacement");
                GC.KeepAlive(recoveredFixture);
            }
            editor.Undo();
            Equal(baseline, editor.GetText(editor.Length + 1), "journaled replacement supports one-step undo after an aborted candidate");
        }
        finally
        {
            await editor.StopJournalAsync();
            await folder.DeleteAsync(StorageDeleteOption.PermanentDelete);
            GC.KeepAlive(fixture);
        }
    }

    // NotepadsDialog takes its theme from the app setting, while the application theme follows Windows.
    // The host's App.xaml copies the app's dialog and hyperlink resources; each must follow the dialog's theme.
    private static async Task CheckDialogThemeResourcesAsync()
    {
        foreach (var theme in new[] { ElementTheme.Light, ElementTheme.Dark })
        {
            // Native AOT needs WinRT casts for objects the framework creates.
            var probes = WinRT.CastExtensions.As<StackPanel>(Windows.UI.Xaml.Markup.XamlReader.Load(
                "<StackPanel xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'>" +
                "<Border Background='{ThemeResource HyperlinkForegroundPointerOver}'/>" +
                "<Border Background='{ThemeResource HyperlinkForegroundPressed}'/></StackPanel>"));
            var dialog = new ContentDialog { RequestedTheme = theme, Content = probes, CloseButtonText = "Close" };
            var shown = dialog.ShowAsync().AsTask();
            // Opened can precede the template, so wait for the template's background element.
            Border element = null;
            for (var attempt = 0; element == null && attempt < 250; attempt++)
            {
                await Task.Delay(20);
                element = FindNamedDescendant<Border>(dialog, "BackgroundElement");
            }
            static Windows.UI.Color? ColorOf(object border) =>
                border == null ? null : WinRT.CastExtensions.As<SolidColorBrush>(WinRT.CastExtensions.As<Border>(border).Background).Color;
            var background = ColorOf(element);
            var pointerOver = ColorOf(probes.Children[0]);
            var pressed = ColorOf(probes.Children[1]);
            dialog.Hide();
            await shown;
            var ink = theme == ElementTheme.Light ? (byte)0 : (byte)255;
            Check(background == (theme == ElementTheme.Light ? Windows.UI.Colors.White : Windows.UI.Color.FromArgb(255, 16, 16, 16)) &&
                pointerOver == Windows.UI.Color.FromArgb(0x99, ink, ink, ink) && pressed == Windows.UI.Color.FromArgb(0x66, ink, ink, ink),
                $"a {theme} dialog under the {Application.Current.RequestedTheme} application theme uses its own theme's background " +
                $"and hyperlink colors (background {background}, pointer over {pointerOver}, pressed {pressed})");
        }
    }

    private static T FindNamedDescendant<T>(DependencyObject root, string name) where T : FrameworkElement
    {
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(root); index++)
        {
            var child = VisualTreeHelper.GetChild(root, index);
            if (child is FrameworkElement element && element.Name == name)
                return WinRT.CastExtensions.As<T>(element);
            var nested = FindNamedDescendant<T>(child, name);
            if (nested != null) return nested;
        }
        return null;
    }
}
