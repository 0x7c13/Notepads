// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.Settings;
using NotepadsEditorTests;
using Windows.System;
using Windows.ApplicationModel.Resources;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Documents;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Media;
using WinUIEditor;
using WinRT;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class EditorToolsFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var editor = (TextEditor)host.Core.GetSelectedTab().ReadyEditor;
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var preferences = ApplicationPreferences.EditorReadingAids;
        try
        {
            var settings = await SettingsAsync<TextAndEditorSettingsPage>(host.Page, "TextAndEditor");
            foreach (var name in new[] { "WhitespaceToggle", "IndentGuidesToggle", "BraceMatchingToggle", "OccurrencesToggle" })
            {
                var toggle = Named<ToggleSwitch>(settings, name);
                Check(toggle.Header == null && toggle.OnContent is string label && !string.IsNullOrWhiteSpace(label) &&
                    label == (string)toggle.OffContent && new ToggleSwitchAutomationPeer(toggle).GetName() == label,
                    "Reading aids use inline labels and retain an accessible name in either state.");
            }
            Named<ToggleSwitch>(settings, "WhitespaceToggle").IsOn = true;
            Named<ToggleSwitch>(settings, "IndentGuidesToggle").IsOn = true;
            Named<ToggleSwitch>(settings, "BraceMatchingToggle").IsOn = true;
            Named<ToggleSwitch>(settings, "OccurrencesToggle").IsOn = true;
            Check(ApplicationPreferences.EditorReadingAids == (ReadingAids)15,
                "Real reading-aid toggles persist the complete preference set.");
            await CloseSettingsAsync(host.Page);
            ApplicationPreferences.EditorReadingAids = ReadingAids.Whitespace | ReadingAids.IndentGuides | ReadingAids.Braces | ReadingAids.Occurrences;
            await UntilAsync(() => native.ViewWS == WhiteSpace.VisibleAlways && native.IndentationGuides == IndentView.LookBoth,
                "Reading aid settings reach the actual native editor.");
            await VerifyLongLineFindAllAsync(editor, log, "ASCII", "needle 0123456789 ", "", "needle");
            await VerifyLongLineFindAllAsync(editor, log, "CJK-tail", "needle 0123456789 ", "中", "needle");
            await VerifyLongLineFindAllAsync(editor, log, "Unicode", "needle 中文😀e\u0301 ", "", "中文😀");
            await VerifyGeometryInvalidationAsync(editor);
            await VerifyBraceStyleInvalidationAsync(editor);
            await VerifyOccurrenceReflowAsync(editor);
            core.SelectAll();
            editor.TypeText("(中文😀) 中文😀\r  b\r  a\r  b\rtail");
            native.SetSel(0, 0);
            core.RefreshReadingAids();
            Check(core.TestHighlightedBracePair == (0L, 11L) && native.CurrentPos == 0,
                "Brace highlighting retains UTF-8 geometry across CJK and supplementary characters without moving the caret.");
            native.SetSel(1, 11);
            core.RefreshReadingAids();
            Check(native.IndicatorValueAt(8, 1) != 0 && native.IndicatorValueAt(8, 13) != 0,
                "Selection occurrence indicators preserve Unicode byte positions.");
            var before = editor.GetText();
            native.SetSel(native.PositionFromLine(1), native.PositionFromLine(4));
            await editor.TransformSelectedLinesAsync(SelectedLineOperation.SortAscending);
            Check(editor.GetText() == "(中文😀) 中文😀\r  a\r  b\r  b\rtail",
                "Whole-line sorting excludes the next line and preserves surrounding text.");
            core.Undo();
            Check(editor.GetText() == before, "One undo restores a complete sort.");
            native.SetSel(native.PositionFromLine(1), native.PositionFromLine(4));
            await editor.TransformSelectedLinesAsync(SelectedLineOperation.RemoveDuplicates);
            Check(editor.GetText() == "(中文😀) 中文😀\r  b\r  a\rtail", "Dedup preserves the following line and its separator.");
            core.Undo();
            Check(editor.GetText() == before, "Dedup is a single undo step.");

            var selection = native.SelectionRevision;
            var found = await core.FindAllAsync(new SearchContext("中文😀", true), CancellationToken.None);
            Check(found.Rows.Count == 2 && found.Rows[1].Start == 13 && native.SelectionRevision == selection,
                "Find all returns exact Unicode ranges without moving the selection.");
            Check(core.NavigateSearchResult(found, 1) && core.GetSelectedText() == "中文😀", "Result navigation selects the exact match.");
            editor.ShowFindAndReplaceControl(false);
            var findControl = Named<FindAndReplaceControl>(editor, "FindAndReplaceControl");
            var findRoot = Named<Grid>(findControl, "FindAndReplaceRootGrid");
            await UntilAsync(() => findRoot.IsLoaded && findRoot.KeyboardAccelerators.Any(accelerator => accelerator.Key == VirtualKey.Enter &&
                accelerator.Modifiers == VirtualKeyModifiers.Control && ReferenceEquals(accelerator.ScopeOwner, findRoot)),
                "Find all has Ctrl+Enter scoped to the visible find panel, without depending on its closed flyout.");
            Named<TextBox>(findControl, "FindBar").Text = "中文😀";
            Invoke(Named<MenuFlyoutItem>(findControl, "FindAllButton"));
            await UntilAsync(() => editor.FindName("FindAllList") is ListView list && list.Items.Count == 2,
                "The actual Find all menu opens a populated result list.");
            var resultList = Named<ListView>(editor, "FindAllList");
            await UntilAsync(() => resultList.SelectedIndex == 0 &&
                resultList.ContainerFromIndex(0) is ListViewItem item && item.FocusState == FocusState.Pointer,
                "Completing Find all selects the first result without an initial keyboard focus rectangle.");
            editor.UpdateLayout();
            Check(resultList.ActualHeight > 0 && !string.IsNullOrEmpty(Named<TextBlock>(editor, "FindAllStatus").Text),
                "The result pane is laid out and has a localized accessible status.");
            var resultPanel = Named<Grid>(editor, "FindAllPanel");
            var closeButton = Named<Button>(editor, "FindAllCloseButton");
            var closeIcon = Named<FontIcon>(editor, "FindAllCloseIcon");
            var iconBounds = closeIcon.TransformToVisual(closeButton).TransformBounds(
                new Windows.Foundation.Rect(0, 0, closeIcon.ActualWidth, closeIcon.ActualHeight));
            Check(iconBounds.Width > 0 && iconBounds.Height > 0 && iconBounds.Left >= 0 && iconBounds.Top >= 0 &&
                iconBounds.Right <= closeButton.ActualWidth && iconBounds.Bottom <= closeButton.ActualHeight,
                "The close glyph fits inside its compact button without being clipped by template padding.");
            await VerifyFindAllSizingAsync(host, editor);
            await VerifyFindAllDiffEscapeAsync(editor);
            // This checks the dismissal command only. The UIAutomation desktop
            // case sends real Escape keys through all three focused controls.
            editor.TestEscape();
            await UntilAsync(() => resultPanel.Visibility == Visibility.Collapsed && resultList.Items.Count == 0 && !editor.TestFindVisible,
                "One Escape closes the result list and query bar together.");
            editor.ShowFindAndReplaceControl(false);
            Named<TextBox>(findControl, "FindBar").Text = "中文😀";
            Invoke(Named<MenuFlyoutItem>(findControl, "FindAllButton"));
            await UntilAsync(() => resultList.Items.Count == 2 && resultList.SelectedIndex == 0 &&
                resultList.ContainerFromIndex(0) is ListViewItem first && first.FocusState == FocusState.Pointer,
                "Find all can reopen with a subtle selected result after dismissal.");
            editor.TypeText("changed");
            Check(!core.NavigateSearchResult(found, 0), "Stale results cannot navigate after an edit.");
            Check(resultList.Items.Count == 0 && !editor.TestFindAllVisible, "Editing dismisses obsolete search results immediately.");
            editor.HideFindAndReplaceControl();
            var empty = await core.FindAllAsync(new SearchContext("(?=.)", useRegex: true), CancellationToken.None);
            Check(empty.Rows.Count > 1 && empty.Rows.Select(row => row.Start).Distinct().Count() == empty.Rows.Count,
                "Zero-width regex results advance without looping or splitting Unicode characters.");
            var bad = await core.FindAllAsync(new SearchContext("[", useRegex: true), CancellationToken.None);
            Check(bad.Summary.Status == EditorSearchStatus.InvalidPattern, "Find all shares the existing regex error contract.");
            native.SetSel(0, native.Length);
            editor.TypeText(new string('a', TextEditorCore.MaximumSearchResults + 1));
            var capped = await core.FindAllAsync(new SearchContext("a", true), CancellationToken.None);
            Check(capped.Limited && capped.Rows.Count == TextEditorCore.MaximumSearchResults, "Results are bounded and truncation is explicit.");
            native.SetSel(0, native.Length);
            editor.TypeText(new string('x', 32766) + "中文😀" + " tail");
            var boundary = await core.FindAllAsync(new SearchContext("中文😀", true), CancellationToken.None);
            Check(boundary.Rows.Count == 1 && boundary.Rows[0].Start == 32766, "Literal matches survive UTF-8 slice boundaries.");
            using (var cancel = new CancellationTokenSource())
            {
                var searching = core.FindAllAsync(new SearchContext("not present", true), cancel.Token);
                cancel.Cancel();
                try { await searching; throw new InvalidOperationException("Find all did not observe cancellation between slices."); }
                catch (OperationCanceledException) { }
            }
            await VerifyFindAllSummaryAsync(editor);
            await VerifyFindAllSlicesAsync(editor, log);
            var closing = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Cancelled line operation");
            try
            {
                await closing.InitializeNewAsync();
                closing.TypeText("z\ra\rb");
                closing.TestNativeCore.SelectAll();
                var sorting = closing.TransformSelectedLinesAsync(SelectedLineOperation.SortAscending);
                await host.Core.DiscardUnattachedEditorAsync(closing);
                await sorting;
                Check(closing.DisposalCompletion.IsCompleted, "Closing drains and cancels a pending line operation.");
            }
            finally { await host.Core.DiscardUnattachedEditorAsync(closing); }
        }
        finally { ApplicationPreferences.EditorReadingAids = preferences; }
        log.AppendLine("PASS: reading aids, Unicode brace/occurrence geometry, line undo, find-all Unicode/regex/bounds/stale guards.");
    }

    private static async Task VerifyFindAllSizingAsync(FeatureTestHost host, TextEditor editor)
    {
        var grip = Named<Notepads.Controls.GridSplitter>(editor, "FindAllResizeGrip");
        var gripSurface = (Grid)VisualTreeHelper.GetChild(grip, 0);
        foreach (var state in new[] { "PointerOver", "Pressed", "Normal" })
        {
            VisualStateManager.GoToState(grip, state, false);
            Check(gripSurface.Background is SolidColorBrush brush && brush.Color.A == 0,
                $"The Find All boundary stays transparent in the {state} state.");
        }
        var panel = Named<Grid>(editor, "FindAllPanel");
        var rows = Named<Grid>(editor, "FindAllHost").RowDefinitions;
        var row = rows[1];
        var initialHeight = row.Height.Value;
        var editorHeight = editor.Height;
        var originalTab = host.Core.GetTab(editor);
        var query = Named<TextBox>(Named<FindAndReplaceControl>(editor, "FindAndReplaceControl"), "FindBar").Text;
        var native = editor.TestNativeCore.TestFeatureNative;
        var selection = (native.CurrentPos, native.Anchor, native.DocumentRevision);
        var results = editor.TestFindAllList.ItemsSource;
        Notepads.Presentation.Workspace.WorkspaceTab otherTab = null;
        TextEditor other = null;
        try
        {
            var preferred = Math.Max(row.MinHeight, initialHeight - 24);
            Check(preferred < initialHeight, "The resize fixture has space to adjust its result panel.");
            row.Height = new GridLength(preferred);
            editor.UpdateLayout();
            await UntilAsync(() => Math.Abs(panel.ActualHeight - preferred) < 1,
                "Changing the result row applies the chosen height.");
            Check(selection == (native.CurrentPos, native.Anchor, native.DocumentRevision) &&
                ReferenceEquals(results, editor.TestFindAllList.ItemsSource),
                "Resizing preserves the document, selection and search results.");

            editor.TestEscape();
            editor.UpdateLayout();
            Check(row.Height.Value == 0 && row.MinHeight == 0 && row.ActualHeight == 0,
                "Closing results returns their entire row to the editor.");
            await UntilAsync(() => !editor.TestFindVisible, "The query close animation completes before reopening.");
            await OpenAsync(editor, query, 2);
            await UntilAsync(() => Math.Abs(panel.ActualHeight - preferred) < 1,
                "Reopening results restores the last chosen height.");

            editor.Height = 160;
            editor.UpdateLayout();
            await UntilAsync(() => panel.ActualHeight <= 80 && row.MaxHeight <= 80,
                "A small editor clamps the result panel and retains editing space.");
            Check(editor.TestNativeCore.ActualHeight >= 79,
                "A constrained result panel leaves half the small editor available for text.");
            editor.TestEscape();
            await UntilAsync(() => !editor.TestFindVisible, "The small query panel finishes closing before reopening.");
            await OpenAsync(editor, query, 2);
            editor.Height = editorHeight;
            editor.UpdateLayout();
            await UntilAsync(() => Math.Abs(panel.ActualHeight - preferred) < 1,
                "Temporary size limits and reopening in a small window preserve the preferred height.");

            otherTab = await NewTabAsync(host.Page);
            other = (TextEditor)otherTab.ReadyEditor;
            other.TypeText("cat");
            await OpenAsync(other, "cat", 1);
            var otherPanel = Named<Grid>(other, "FindAllPanel");
            await UntilAsync(() => Math.Abs(otherPanel.ActualHeight - preferred) < 1,
                "New tabs inherit the current run's result height.");
            var nextHeight = preferred + 8;
            Named<Grid>(other, "FindAllHost").RowDefinitions[1].Height = new GridLength(nextHeight);
            host.Core.SwitchTo(originalTab);
            await UntilAsync(() => editor.IsLoaded && Math.Abs(panel.ActualHeight - nextHeight) < 1,
                "Returning to an existing tab uses the most recently adjusted height.");
        }
        finally
        {
            if (otherTab != null) host.Core.DeleteTab(otherTab);
            if (other != null) await other.DisposalCompletion;
            host.Core.SwitchTo(originalTab);
            editor.Height = editorHeight;
            editor.UpdateLayout();
            row.Height = new GridLength(initialHeight);
        }

        static async Task OpenAsync(TextEditor target, string text, int count)
        {
            target.ShowFindAndReplaceControl(false);
            var find = Named<FindAndReplaceControl>(target, "FindAndReplaceControl");
            var query = Named<TextBox>(find, "FindBar");
            query.Text = text;
            var command = Named<MenuFlyoutItem>(find, "FindAllButton");
            await UntilAsync(() => query.IsLoaded && query.FocusState != FocusState.Unfocused && command.IsEnabled,
                "The query is loaded and focused before invoking Find All.");
            Invoke(command);
            await UntilAsync(() => target.TestFindAllList?.Items.Count == count &&
                target.TestFindAllList.SelectedIndex == 0 &&
                target.TestFindAllList.ContainerFromIndex(0) is ListViewItem first && first.FocusState == FocusState.Pointer,
                "The result panel finishes opening before the next sizing action.");
        }
    }

    private static async Task VerifyLongLineFindAllAsync(TextEditor editor, StringBuilder log,
        string fixture, string unit, string suffix, string query)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var wrapping = core.TextWrapping;
        var originalText = editor.GetText();
        var readingAids = ApplicationPreferences.EditorReadingAids;
        var payload = string.Concat(Enumerable.Repeat(unit, 2000)) + suffix;
        var text = "[\r" + string.Join(",\r", Enumerable.Range(0, 1500).Select(i =>
            "{\"value\":\"" + (i % 50 == 0 ? payload : "short") + "\"}")) + "\r]";
        try
        {
            core.TextWrapping = TextWrapping.NoWrap;
            core.SelectAll();
            editor.TypeText(text);
            editor.SetLanguageOverride("json");
            native.SetSel(0, 0);
            await EditorTestLayout.AfterPaintAsync(native, () => native.FirstVisibleLine = 45);
            var baselineCpu = await MeasureScrollAsync("closed");

            editor.ShowFindAndReplaceControl(false);
            var find = Named<FindAndReplaceControl>(editor, "FindAndReplaceControl");
            Named<TextBox>(find, "FindBar").Text = query;
            var findAll = Named<MenuFlyoutItem>(find, "FindAllButton");
            await UntilAsync(() => findAll.IsEnabled, "The loaded query enables Find All.");
            Invoke(findAll);
            await UntilAsync(() => editor.TestFindAllList.Items.Count == TextEditorCore.MaximumSearchResults,
                "Find All bounds a long-line JSON result set.");
            Check(editor.TestFindAllList.Items.Cast<DocumentSearchRow>().All(row => row.Label.Length < 280),
                "Long-line result previews remain bounded before XAML layout.");
            await MeasureScrollAsync("results");

            var matches = await core.FindAllAsync(new SearchContext(query, true), CancellationToken.None);
            Check(core.NavigateSearchResult(matches, 0) && core.GetSelectedText() == query,
                "Long-line results navigate to the exact match.");
            await EditorTestLayout.AfterPaintAsync(native, () => native.FirstVisibleLine = 45);
            core.RefreshReadingAids();
            var offsetInLine = matches.Rows[0].Start - native.PositionFromLine(1);
            Check(native.IndicatorValueAt(8, native.PositionFromLine(51) + offsetInLine) != 0,
                "Occurrences on a visible long JSON line remain highlighted.");
            var selection = (native.CurrentPos, native.Anchor, native.DocumentRevision);
            var selectedCpu = await MeasureScrollAsync("selected-result");
            await EditorTestLayout.AfterPaintAsync(native, () => native.FirstVisibleLine = 0);
            ApplicationPreferences.EditorReadingAids = readingAids & ~ReadingAids.Occurrences;
            core.RefreshReadingAids();
            var selectionOnlyCpu = await MeasureScrollAsync("selected-line-baseline");
            ApplicationPreferences.EditorReadingAids = readingAids;
            core.RefreshReadingAids();
            var visibleCpu = await MeasureScrollAsync("selected-line-visible");
            // CPU time excludes waits caused by other processes. Warm each phase
            // and use medians to tolerate transient work in the test application.
            // The relative allowance scales with the machine; the absolute floor
            // accommodates coarse process counters and a very cheap baseline.
            var cpuBudget = Math.Max(500, baselineCpu * 8);
            // A visible selection has its own shaping cost. Compare its highlights
            // against the same selection and viewport with occurrences disabled.
            var visibleBudget = Math.Max(500, selectionOnlyCpu * 3);
            Check(selectedCpu <= cpuBudget && visibleCpu <= visibleBudget,
                $"{fixture} long-line highlight CPU budget: through={selectedCpu:F1}/{cpuBudget:F1}, visible={visibleCpu:F1}/{visibleBudget:F1} ms per four paints.");
            Check(selection == (native.CurrentPos, native.Anchor, native.DocumentRevision),
                "Scrolling long-line search results preserves selection and document revision.");
            // Pan to a distant match so inverse hit testing must resolve a long
            // Unicode prefix. Use the real caret and mouse-coordinate APIs.
            var distant = matches.Rows[^1];
            var lineStart = native.PositionFromLine(native.LineFromPosition(distant.Start));
            var pan = Math.Max(0, native.PointXFromPosition(distant.Start) - native.PointXFromPosition(lineStart) - 40);
            await EditorTestLayout.AfterPaintAsync(native, () => native.XOffset = pan);
            using var geometryProcess = System.Diagnostics.Process.GetCurrentProcess();
            var geometryCpu = geometryProcess.TotalProcessorTime;
            var geometryWatch = System.Diagnostics.Stopwatch.StartNew();
            for (var i = 0; i < 16; i++)
            {
                var position = i % 2 == 0 ? distant.Start : distant.End;
                var x = (int)native.PointXFromPosition(position);
                var y = (int)(native.PointYFromPosition(position) + native.TextHeight(1) / 2);
                Check(native.PositionFromPointClose(x, y) == position,
                    "Distant mouse hits round-trip through actual long-line caret geometry.");
            }
            var geometryMilliseconds = (geometryProcess.TotalProcessorTime - geometryCpu).TotalMilliseconds;
            log.AppendLine($"MEASURE: long-line {fixture}/geometry, 16 caret/hit pairs, wall={geometryWatch.Elapsed.TotalMilliseconds:F1} ms, CPU={geometryMilliseconds:F1} ms.");
            Check(geometryMilliseconds <= Math.Max(500, baselineCpu * 8),
                $"{fixture} long-line geometry CPU budget exceeded: {geometryMilliseconds:F1} ms.");
            Check(selection == (native.CurrentPos, native.Anchor, native.DocumentRevision),
                "Hit testing leaves the document and selection unchanged.");
        }
        finally
        {
            ApplicationPreferences.EditorReadingAids = readingAids;
            native.XOffset = 0;
            editor.HideFindAndReplaceControl();
            native.SetSel(0, 0);
            core.RefreshReadingAids();
            core.SelectAll();
            editor.TypeText(originalText);
            core.TextWrapping = wrapping;
        }

        async Task<double> MeasureScrollAsync(string phase)
        {
            using var process = System.Diagnostics.Process.GetCurrentProcess();
            await PaintBatchAsync();
            var cpuSamples = new double[3];
            var wallSamples = new double[3];
            for (var sample = 0; sample < cpuSamples.Length; sample++)
            {
                var cpu = process.TotalProcessorTime;
                var watch = System.Diagnostics.Stopwatch.StartNew();
                await PaintBatchAsync();
                cpuSamples[sample] = (process.TotalProcessorTime - cpu).TotalMilliseconds;
                wallSamples[sample] = watch.Elapsed.TotalMilliseconds;
            }
            Array.Sort(cpuSamples);
            Array.Sort(wallSamples);
            log.AppendLine($"MEASURE: long-line Find All {fixture}/{phase}, median of 3 four-paint batches, wall={wallSamples[1]:F1} ms, CPU={cpuSamples[1]:F1} ms.");
            return cpuSamples[1];
        }

        async Task PaintBatchAsync()
        {
            for (var i = 0; i < 4; i++)
                await EditorTestLayout.AfterPaintAsync(native, () => native.LineScroll(0, i % 2 == 0 ? 1 : -1));
        }
    }

    private static async Task VerifyGeometryInvalidationAsync(TextEditor editor)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var originalText = editor.GetText();
        var width = core.Width;
        var fontSize = core.FontSize;
        var wrapping = core.TextWrapping;
        var tabWidth = native.TabWidth;
        var zoom = native.Zoom;
        var cache = native.LayoutCache;
        var representation = native.GetRepresentation("\u0001");
        var representationAppearance = native.GetRepresentationAppearance("\u0001");
        try
        {
            editor.SetLanguageOverride("plaintext");
            core.TextWrapping = TextWrapping.NoWrap;
            core.SelectAll();
            editor.TypeText("a\t中文😀e\u0301\u0001 tail " + new string('x', 80) + "\rsecond");
            native.SetSel(0, 0);
            Verify("initial geometry");
            native.ClearRepresentation("\u0001");
            Verify("clear representation");
            native.SetRepresentation("\u0001", representation);
            Verify("restore representation");
            native.SetRepresentation("\u0001", "CONTROL");
            Verify("wider representation");
            native.SetRepresentationAppearance("\u0001", RepresentationAppearance.Plain);
            Verify("representation appearance");
            native.ClearAllRepresentations();
            Verify("reset representations");
            foreach (var change in new (string Name, Action Apply)[]
            {
                ("edit", () => { native.SetSel(0, 1); editor.TypeText("WWW"); }),
                ("undo", () => core.Undo()),
                ("font size", () => core.FontSize = fontSize + 4),
                ("tab width", () => native.TabWidth = tabWidth + 3),
                ("zoom", () => native.Zoom = zoom + 2),
                ("wrap", () => { core.TextWrapping = TextWrapping.Wrap; core.Width = 250; core.UpdateLayout(); }),
                ("resize", () => { core.Width = 500; core.UpdateLayout(); }),
                ("horizontal pan", () => { core.TextWrapping = TextWrapping.NoWrap; native.XOffset = 17; })
            })
            {
                await EditorTestLayout.AfterPaintAsync(native, change.Apply);
                Verify(change.Name);
            }
        }
        finally
        {
            native.LayoutCache = cache;
            native.TabWidth = tabWidth;
            native.Zoom = zoom;
            native.XOffset = 0;
            if (string.IsNullOrEmpty(representation)) native.ClearRepresentation("\u0001");
            else native.SetRepresentation("\u0001", representation);
            native.SetRepresentationAppearance("\u0001", representationAppearance);
            core.Width = width;
            core.FontSize = fontSize;
            core.TextWrapping = wrapping;
            core.SelectAll();
            editor.TypeText(originalText);
            core.UpdateLayout();
        }

        void Verify(string change)
        {
            // Compare the actual warmed geometry with independently rebuilt
            // layouts. Include interior UTF-8 offsets as well as valid boundaries.
            var cached = Capture();
            native.LayoutCache = LineCache.None;
            var fresh = Capture();
            native.LayoutCache = cache;
            Check(cached.SequenceEqual(fresh), $"Cached geometry matches fresh layouts after {change}.");
            _ = Capture();
        }

        (long X, long Y)[] Capture() => Enumerable.Range(0, checked((int)native.Length + 1))
            .Select(position => ((long)native.PointXFromPosition(position), (long)native.PointYFromPosition(position))).ToArray();
    }

    private static async Task VerifyFindAllSlicesAsync(TextEditor editor, StringBuilder log)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        void SetText(string text) { native.SetSel(0, native.Length); editor.TypeText(text); }

        SetText(new string('a', 256 * 1024));
        var query = new string('a', (int)TextEditorCore.SearchPatternLimit - 1) + "b";
        foreach (var matchCase in new[] { false, true })
        {
            using var cancel = new CancellationTokenSource();
            var elapsed = System.Diagnostics.Stopwatch.StartNew();
            var searching = core.FindAllAsync(new SearchContext(query, matchCase), cancel.Token);
            Check(!searching.IsCompleted, "Long-prefix Find All yields before exhausting the document.");
            // Dispatch to the actual UI queue, so a synchronous search cannot
            // pass by merely executing cancellation on a background thread.
            await editor.Dispatcher.RunAsync(Windows.UI.Core.CoreDispatcherPriority.Normal, () => cancel.Cancel());
            try { await searching; throw new InvalidOperationException("Long-prefix search ignored cancellation."); }
            catch (OperationCanceledException) { }
            Check(elapsed.Elapsed < TimeSpan.FromSeconds(2), "Long-prefix Find All services UI cancellation within its overall time budget.");
            log.AppendLine($"MEASURE: long-prefix Find All cancellation (case={matchCase}): {elapsed.Elapsed.TotalMilliseconds:F1} ms.");
        }

        SetText("ß SS ss\r中文😀 \0\r" + new string('x', 32760) + " 中文😀 ");
        native.SetTargetRange(2, 5);
        native.SearchFlags = FindOption.WordStart;
        var selection = native.SelectionRevision;
        var matches = await core.FindAllAsync(new SearchContext("SS", matchWholeWord: true), CancellationToken.None);
        Check(matches.Rows.Count == 3 && matches.Rows[0].Start == 0 && matches.Rows[0].End == 2,
            "Sliced literal search retains Unicode case-fold expansion and whole-word matching.");
        matches = await core.FindAllAsync(new SearchContext("中文😀", true), CancellationToken.None);
        Check(matches.Rows.Count == 2, "Sliced search finds complete Unicode matches across candidate boundaries.");
        matches = await core.FindAllAsync(new SearchContext("\0", true), CancellationToken.None);
        Check(matches.Rows.Count == 1 && !matches.Limited && native.TargetStart == 2 && native.TargetEnd == 5 &&
            native.SearchFlags == FindOption.WordStart && native.SelectionRevision == selection,
            "Literal slices preserve NUL, terminate at EOF, and leave native search/selection state intact.");
    }

    private static async Task VerifyFindAllDiffEscapeAsync(TextEditor editor)
    {
        var text = editor.GetText();
        var results = editor.TestFindAllList.ItemsSource;
        var core = editor.TestNativeCore;
        await editor.OpenSideBySideDiffViewerAsync();
        try
        {
            await UntilAsync(() => editor.Mode == TextEditorMode.DiffPreview && core.ActualHeight == 0,
                "Diff hides the source while its Find All results remain open.");
            editor.TestEscape();
            Check(editor.Mode == TextEditorMode.Editing && core.IsEnabled && editor.TestFindAllVisible &&
                ReferenceEquals(editor.TestFindAllList.ItemsSource, results),
                "One Escape closes Diff and retains the source's Find All results.");
        }
        finally { editor.CloseSideBySideDiffViewer(); }

        var preparing = editor.OpenSideBySideDiffViewerAsync();
        try
        {
            Check(editor.TestDiffPreparing, "The second Diff is still preparing before the UI thread yields.");
            editor.TestEscape();
            Check(!editor.TestDiffPreparing && editor.TestFindAllVisible,
                "Escape cancels Diff preparation before dismissing the visible result panel.");
            await preparing;
            Check(editor.Mode == TextEditorMode.Editing && core.IsEnabled &&
                ReferenceEquals(editor.TestFindAllList.ItemsSource, results) && editor.GetText() == text,
                "Canceled preparation drains without publishing Diff or changing results and source text.");
        }
        finally
        {
            editor.CloseSideBySideDiffViewer();
            await preparing;
        }
    }

    private static async Task VerifyFindAllSummaryAsync(TextEditor editor)
    {
        var resources = ResourceLoader.GetForCurrentView();
        var native = editor.TestNativeCore.TestFeatureNative;
        editor.ShowFindAndReplaceControl(false);
        var find = Named<FindAndReplaceControl>(editor, "FindAndReplaceControl");
        var query = Named<TextBox>(find, "FindBar");
        var list = Named<ListView>(editor, "FindAllList");
        var status = Named<TextBlock>(editor, "FindAllStatus");
        var full = Named<TextBlock>(editor, "FindAllFullSummary");
        var rules = Named<Run>(editor, "FindAllRulesText");
        var panel = Named<Grid>(editor, "FindAllPanel");
        var header = Named<Grid>(editor, "FindAllSummaryHeader");
        // The attached property's IInspectable needs an explicit AOT projection.
        var tooltip = ToolTipService.GetToolTip(header).As<ToolTip>();
        var previousWidth = panel.Width;
        var previousFlow = panel.FlowDirection;

        void SetText(string text) { native.SetSel(0, native.Length); editor.TypeText(text); }
        async Task SearchAsync(string text, int? count, bool matchCase = false, bool wholeWord = false, bool regex = false, bool limited = false)
        {
            Named<ToggleMenuFlyoutItem>(find, "MatchCaseToggle").IsChecked = matchCase;
            Named<ToggleMenuFlyoutItem>(find, "MatchWholeWordToggle").IsChecked = wholeWord;
            Named<ToggleMenuFlyoutItem>(find, "UseRegexToggle").IsChecked = regex;
            query.Text = text;
            Invoke(Named<MenuFlyoutItem>(find, "FindAllButton"));
            var expected = count == null ? resources.GetString("EditorTools_FindAllErrorTitle") :
                string.Format(resources.GetString(limited ? "EditorTools_FindAllPartialCount" : count == 1
                    ? "EditorTools_FindAllOne" : "EditorTools_FindAllCount"), count);
            await UntilAsync(() => status.Text == expected && list.Items.Count == (count ?? 0),
                "The result summary reflects the completed search, including singular, zero, partial and error states.");
            // The command restores focus after publishing its results. Start
            // the next interaction only after that completion step has run.
            if (count > 0)
                await UntilAsync(() => list.SelectedIndex == 0 && list.ContainerFromIndex(0) is ListViewItem first &&
                    first.FocusState != FocusState.Unfocused,
                    "The completed Find All command has focused its first result before the next interaction.");
        }

        try
        {
            SetText("alpha Alpha alphabet\ralpha");
            await SearchAsync("alpha", 4);
            Check(full.Text.Contains("alpha", StringComparison.Ordinal) &&
                full.Text.Contains(resources.GetString("EditorTools_FindAllAnyWord"), StringComparison.Ordinal) &&
                rules.Text.Contains(resources.GetString("EditorTools_FindAllCaseInsensitive"), StringComparison.Ordinal),
                "The summary identifies the literal query, default case rule and full word-boundary policy.");
            await SearchAsync("alpha", 2, matchCase: true, wholeWord: true);
            Check(rules.Text.Contains(resources.GetString("EditorTools_FindAllWholeWord"), StringComparison.Ordinal) &&
                rules.Text.Contains(resources.GetString("EditorTools_FindAllCaseSensitive"), StringComparison.Ordinal),
                "Case-sensitive whole-word results advertise the effective rules.");
            await SearchAsync("a[a-z]+", 4, regex: true);
            Check(rules.Text.Contains(resources.GetString("EditorTools_FindAllRegex"), StringComparison.Ordinal) &&
                !full.Text.Contains(resources.GetString("EditorTools_FindAllAnyWord"), StringComparison.Ordinal),
                "Regex summaries do not advertise an inapplicable whole-word option.");
            query.Focus(FocusState.Programmatic);
            query.Text = "different query";
            await UntilAsync(() => !editor.TestFindAllVisible && list.Items.Count == 0,
                "Changing the query dismisses the old results.");
            Check(editor.TestFindVisible && query.FocusState != FocusState.Unfocused,
                "Changing the query keeps the search box open and preserves typing focus.");
            await SearchAsync("[", null, regex: true);
            Check(full.Text.Contains(resources.GetString("FindAndReplace_NotificationMsg_InvalidRegex"), StringComparison.Ordinal),
                "The full summary explains the actual search error.");

            SetText("alpha");
            await SearchAsync("alpha", 1);
            await SearchAsync("missing", 0);
            editor.UpdateLayout();
            var headerHeight = header.ActualHeight;
            var longQuery = new string('a', 159) + "😀e\u0301中文" + new string('b', 4096);
            SetText(longQuery + "\r" + longQuery);
            await SearchAsync(longQuery, 2);
            var preview = Named<Run>(editor, "FindAllQueryText").Text;
            Check(preview.Length < longQuery.Length && !preview.Any(char.IsSurrogate) &&
                full.Text.Contains(longQuery, StringComparison.Ordinal) && AutomationProperties.GetName(status) == full.Text,
                "Long Unicode queries have a bounded, intact preview and full tooltip/accessibility text.");
            panel.Width = 320;
            foreach (var flow in new[] { FlowDirection.LeftToRight, FlowDirection.RightToLeft })
            {
                panel.FlowDirection = flow;
                editor.UpdateLayout();
                var close = Named<Button>(editor, "FindAllCloseButton");
                var bounds = close.TransformToVisual(panel).TransformBounds(new Windows.Foundation.Rect(0, 0, close.ActualWidth, close.ActualHeight));
                Check(Math.Abs(header.ActualHeight - headerHeight) < 1 && !status.IsTextTrimmed &&
                    Named<TextBlock>(editor, "FindAllDetails").IsTextTrimmed && bounds.Left >= 0 && bounds.Right <= panel.ActualWidth,
                    "A long query in a narrow LTR/RTL pane stays on one line and keeps the count and close button visible.");
            }
            tooltip.IsOpen = true;
            await UntilAsync(() => tooltip.Content.As<ScrollViewer>().ScrollableHeight > 0,
                "The full long-query tooltip can scroll instead of clipping its conditions.");
            tooltip.IsOpen = false;
            panel.Width = previousWidth;
            panel.FlowDirection = previousFlow;
            SetText(new string('a', TextEditorCore.MaximumSearchResults + 1));
            await SearchAsync("a", TextEditorCore.MaximumSearchResults, limited: true);
            editor.UpdateLayout();
            var scrollbar = Descendants(list).OfType<Windows.UI.Xaml.Controls.Primitives.ScrollBar>()
                .Single(bar => bar.Orientation == Orientation.Vertical && bar.Visibility == Visibility.Visible);
            await UntilAsync(() => scrollbar.ActualWidth > 0, "The Find All vertical scrollbar is arranged.");
            var barBounds = scrollbar.TransformToVisual(panel).TransformBounds(
                new Windows.Foundation.Rect(0, 0, scrollbar.ActualWidth, scrollbar.ActualHeight));
            var listBounds = list.TransformToVisual(panel).TransformBounds(
                new Windows.Foundation.Rect(0, 0, list.ActualWidth, list.ActualHeight));
            // WinUI's scrollbar template keeps a one-DIP inset of its own.
            // The list container must add no further space at the panel edge.
            Check(Math.Abs(listBounds.Right - panel.ActualWidth) < 0.01 &&
                Math.Abs(barBounds.Right - panel.ActualWidth) <= 1.01,
                $"The Find All vertical scrollbar reaches the panel edge while result items retain their inset (bar={barBounds}, panel={panel.ActualWidth}, list={list.ActualWidth}, flow={panel.FlowDirection}).");
            Check(full.Text.Contains(string.Format(resources.GetString("EditorTools_FindAllLimited"), TextEditorCore.MaximumSearchResults), StringComparison.Ordinal),
                "Limited results never present the displayed count as the complete total.");

            SetText(new string('a', 256 * 1024));
            query.Text = new string('a', (int)TextEditorCore.SearchPatternLimit - 1) + "b";
            Invoke(Named<MenuFlyoutItem>(find, "FindAllButton"));
            Check(editor.TestSearchPending && editor.TestFindAllVisible, "A long search remains pending before changing its query.");
            query.Focus(FocusState.Programmatic);
            query.Text = "replacement";
            await UntilAsync(() => !editor.TestSearchPending, "Changing the query drains the canceled search.");
            Check(!editor.TestFindAllVisible && list.Items.Count == 0 && editor.TestFindVisible && query.FocusState != FocusState.Unfocused,
                "A canceled search cannot reopen its results or steal input focus.");

            SetText("alpha alpha");
            await SearchAsync("alpha", 2);
            Invoke(Named<Button>(editor, "FindAllCloseButton"));
            await UntilAsync(() => !editor.TestFindVisible && !editor.TestFindAllVisible,
                "The results close button dismisses the whole search interface.");
        }
        finally
        {
            tooltip.IsOpen = false;
            panel.Width = previousWidth;
            panel.FlowDirection = previousFlow;
            editor.TestEscape();
            editor.HideFindAndReplaceControl();
        }
    }

    private static async Task VerifyOccurrenceReflowAsync(TextEditor editor)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        var width = core.Width;
        var height = core.Height;
        var fontSize = core.FontSize;
        var wrapping = core.TextWrapping;
        try
        {
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                core.Width = 320;
                core.Height = 240;
                core.FontSize = 14;
                core.TextWrapping = TextWrapping.Wrap;
                core.SelectAll();
                editor.TypeText(string.Join('\r', Enumerable.Repeat("cat " + new string('x', 108), 80)));
                native.SetSel(0, 3);
                native.FirstVisibleLine = 0;
                core.UpdateLayout();
            });
            await UntilAsync(() => native.WrapCount(0) > 2 && native.IndicatorValueAt(8, 0) != 0,
                "The narrow wrapped viewport highlights the selected word automatically.");
            var wrapCount = native.WrapCount(0);
            var viewport = (native.FirstVisibleLine, native.LinesOnScreen, native.XOffset);
            var selection = (core.ContentVersion, native.DocumentRevision, native.CurrentPos, native.Anchor);
            var newlyVisibleLine = native.DocLineFromVisible(native.FirstVisibleLine + native.LinesOnScreen + 1) + 1;
            var newlyVisiblePosition = native.PositionFromLine(newlyVisibleLine);
            Check(newlyVisibleLine > 0 && newlyVisiblePosition > 0 && native.IndicatorValueAt(8, newlyVisiblePosition) == 0,
                "The regression match initially lies beyond the sampled document lines.");

            await EditorTestLayout.AfterPaintAsync(native, () => { core.Width = 640; core.UpdateLayout(); });
            Check(native.WrapCount(0) < wrapCount &&
                native.VisibleFromDocLine(newlyVisibleLine) < native.FirstVisibleLine + native.LinesOnScreen,
                "Widening alone exposes the previously unsampled match.");
            Check((native.FirstVisibleLine, native.LinesOnScreen, native.XOffset) == viewport &&
                (core.ContentVersion, native.DocumentRevision, native.CurrentPos, native.Anchor) == selection,
                "Reflow preserves the old cache inputs: viewport rows, scroll position, text and selection.");
            // Do not explicitly refresh: the actual viewport notification must schedule this.
            await UntilAsync(() => native.IndicatorValueAt(8, newlyVisiblePosition) != 0,
                "Width-only reflow automatically highlights the newly visible occurrence.");

            var lastLine = native.DocLineFromVisible(native.FirstVisibleLine + native.LinesOnScreen + 1);
            var mask = native.ModEventMask;
            var indicatorChanges = 0;
            void OnModified(Editor sender, ModifiedEventArgs args)
            {
                if ((args.ModificationType & (int)ModificationFlags.ChangeIndicator) != 0) indicatorChanges++;
            }
            native.Modified += OnModified;
            native.ModEventMask |= ModificationFlags.ChangeIndicator;
            try
            {
                await EditorTestLayout.AfterPaintAsync(native, () => { core.Width = 641; core.UpdateLayout(); });
                Check(native.DocLineFromVisible(native.FirstVisibleLine + native.LinesOnScreen + 1) == lastLine,
                    "A small width change retains the same occurrence sample range.");
                core.RefreshReadingAids();
                Check(indicatorChanges == 0 && native.IndicatorValueAt(8, newlyVisiblePosition) != 0,
                    "Reflow with the same sampled range reuses existing indicators.");
            }
            finally
            {
                native.ModEventMask = mask;
                native.Modified -= OnModified;
            }

            await EditorTestLayout.AfterPaintAsync(native, () => { core.Width = 320; core.UpdateLayout(); });
            await UntilAsync(() => native.IndicatorValueAt(8, newlyVisiblePosition) == 0 && native.IndicatorValueAt(8, 0) != 0,
                "Narrowing drops matches outside the sampled range while retaining the visible selection highlight.");
            Check((core.ContentVersion, native.DocumentRevision, native.CurrentPos, native.Anchor) == selection,
                "Occurrence refresh across repeated reflow does not edit the text or move the selection.");
        }
        finally
        {
            core.Width = width;
            core.Height = height;
            core.FontSize = fontSize;
            core.TextWrapping = wrapping;
            core.UpdateLayout();
        }
    }

    private static async Task VerifyBraceStyleInvalidationAsync(TextEditor editor)
    {
        var core = editor.TestNativeCore;
        var native = core.TestFeatureNative;
        editor.SetLanguageOverride("plaintext");
        editor.TypeText("(\")\") cat cat");
        native.SetSel(0, 0);
        await UntilAsync(() => core.TestHighlightedBracePair == (0L, 2L),
            "Plain text highlights the first closing bracket without moving the caret.");
        var version = core.ContentVersion;
        var revision = native.DocumentRevision;
        editor.SetLanguageOverride("csharp");
        await UntilAsync(() => core.TestHighlightedBracePair == (0L, 4L),
            "Changing language refreshes the applied highlight and skips the quoted bracket.");

        var stringStyle = native.GetStyleAt(2);
        var operatorStyle = native.GetStyleAt(0);
        Check(stringStyle != operatorStyle, "The fixture has distinct string and operator styles.");
        var styledEnd = native.EndStyled;
        native.StartStyling(2, 0);
        native.SetStyling(1, operatorStyle);
        // Keep the completed range: otherwise the next paint asks the lexer
        // to overwrite this controlled late-style notification immediately.
        native.StartStyling(styledEnd, 0);
        await UntilAsync(() => core.TestHighlightedBracePair == (0L, 2L),
            "A later style notification alone refreshes the applied pair.");
        native.StartStyling(2, 0);
        native.SetStyling(1, stringStyle);
        native.StartStyling(styledEnd, 0);
        await UntilAsync(() => core.TestHighlightedBracePair == (0L, 4L),
            "Finishing styling corrects the pair without another text or caret event.");

        native.SetSel(6, 9);
        core.RefreshReadingAids();
        Check(native.IndicatorValueAt(8, 6) != 0 && native.IndicatorValueAt(8, 10) != 0,
            "The style-only performance fixture has existing occurrence ranges.");
        var mask = native.ModEventMask;
        var indicatorChanges = 0;
        void OnModified(Editor sender, ModifiedEventArgs args)
        {
            if ((args.ModificationType & (int)ModificationFlags.ChangeIndicator) != 0) indicatorChanges++;
        }
        native.Modified += OnModified;
        native.ModEventMask |= ModificationFlags.ChangeIndicator;
        try
        {
            native.StartStyling(2, 0);
            native.SetStyling(1, operatorStyle);
            native.StartStyling(styledEnd, 0);
            core.RefreshReadingAids();
            Check(indicatorChanges == 0 && native.IndicatorValueAt(8, 10) != 0,
                "Style-only refresh does not clear or rebuild occurrence indicators.");
        }
        finally
        {
            native.ModEventMask = mask;
            native.Modified -= OnModified;
        }
        native.StartStyling(2, 0);
        native.SetStyling(1, stringStyle);
        native.StartStyling(styledEnd, 0);
        native.SetSel(0, 0);
        core.RefreshReadingAids();
        Check(core.TestHighlightedBracePair == (0L, 4L), "C# pairing is restored before changing back to plain text.");
        editor.SetLanguageOverride("plaintext");
        await UntilAsync(() => core.TestHighlightedBracePair == (0L, 2L),
            "Returning to plain text also invalidates the applied pair.");
        Check(core.ContentVersion == version && native.DocumentRevision == revision && native.CurrentPos == 0 && native.Anchor == 0,
            "Style invalidation preserves text revision and selection.");
    }
}
