// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class MarkdownHtmlTablesFixture
{
    internal const string Caption = "HTML 表格与跨列";
    internal const string SpanText = "跨两列，& 实体测试";
    private const string LongPiece = "合并单元格的长内容用于检查两列合并后的真实换行宽度，继续保留每个来源字符和选区。";
    internal const string LongSpanText = SpanText + " 中😀 " + LongPiece + LongPiece + LongPiece + LongPiece;
    internal const string DiskSource =
        "<table><caption></caption><tbody><tr><td></td></tr></tbody></table>\r\n\r\n" +
        "<table>\r\n<caption>" + Caption + "</caption>\r\n<thead>\r\n" +
        "<tr><th scope=\"col\">列一</th><th scope=\"col\">列二</th></tr>\r\n</thead>\r\n<tbody>\r\n" +
        "<tr><td colspan=\"2\">跨两列，&amp; 实体测试</td></tr>\r\n" +
        "<tr><td>R2C1</td><td><code>x &lt; y</code></td></tr>\r\n</tbody>\r\n</table>";
    internal static readonly string Source = DiskSource.Replace("\r\n", "\r");
    internal static readonly string LongSource = Source.Replace("跨两列，&amp; 实体测试",
        "跨两列，&amp; 实体测试" + LongSpanText[SpanText.Length..], StringComparison.Ordinal);
    internal const string Main = "\n\n" + Caption + "\t\n列一\t列二\n" + SpanText + "\t\nR2C1\tx < y";
    internal static readonly string LongMain = Main.Replace(SpanText, LongSpanText, StringComparison.Ordinal);
    internal const string RichCaption = "Rows and values";
    internal const string RichSource =
        "<table><caption>Rows <a href=\"#caption-target\">and values</a></caption><tbody><tr><td>A</td><th>B</th></tr>" +
        "<tr><th>row key</th><td><a href=\"https://example.test/table\">native link</a> " +
        "<img src=\"missing-table.png\" alt=\"Table alt\"></td></tr></tbody></table>";
    internal const string RichMain = RichCaption + "\t\nA\tB\nrow key\tnative link \ufffc";
}

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunHtmlTablesAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var diskBytes = Encoding.UTF8.GetBytes(MarkdownHtmlTablesFixture.DiskSource);
        var editor = await OpenAsync(host, "html-tables.md", diskBytes);
        var core = editor.TestNativeCore;
        var source = editor.GetText();
        var bytes = ReadCurrentBytes(core);
        Check(source == MarkdownHtmlTablesFixture.Source && bytes.SequenceEqual(Encoding.UTF8.GetBytes(MarkdownHtmlTablesFixture.Source)),
            "HTML table disk CRLF opens as exact independently authored canonical CR current editor bytes.");
        await BytesAsync(editor.EditingFile, diskBytes, "Opening HTML tables preserves authored CRLF disk bytes.");
        log.AppendLine($"OBS: Markdown.HtmlTables current source utf8Length={bytes.Length}, sha256={Convert.ToHexString(SHA256.HashData(bytes))}, " +
            $"CR={source.Count(value => value == '\r')}, LF={source.Count(value => value == '\n')}; diskLength={diskBytes.Length}.");
        var originalUndo = core.CanUndo;
        var originalModified = editor.IsModified;
        var native = core.TestFeatureNative;
        native.SetEmptySelection(0);
        var sourceSelection = (native.Anchor, native.CurrentPos);
        editor.ShowHideContentPreview();
        var openingExtension = editor.TestMarkdownPreview;
        Check(openingExtension != null, "The HTML table fixture opens its actual preview extension synchronously.");
        openingExtension.FollowSource = false;
        var extension = await CurrentAsync(editor);
        Check(ReferenceEquals(extension, openingExtension) && !extension.FollowSource && native.Anchor == 0 && native.CurrentPos == 0,
            "The HTML table fixture fixes the source caret and disables source follow before yielding for preview publication.");
        var preview = extension.PreviewView;
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
        Check(preview.DisplayedText == MarkdownHtmlTablesFixture.Main && text.DocumentRange.GetText(-1) == MarkdownHtmlTablesFixture.Main,
            "The actual table Main/UIA is one caption plus rectangular TSV, with covered tabs retained and each authored text once.");
        var captionedGroup = peer.GetChildren().Single(child => child.GetAutomationControlType() == AutomationControlType.Group &&
            child.GetName() == MarkdownHtmlTablesFixture.Caption);
        var captionPeer = captionedGroup.GetLabeledBy();
        var captionText = captionPeer?.GetPattern(PatternInterface.TextChild) as ITextChildProvider;
        Check(captionPeer != null && captionPeer.GetAutomationControlType() == AutomationControlType.Text &&
            captionPeer.GetName() == MarkdownHtmlTablesFixture.Caption &&
            captionText?.TextRange.GetText(-1) == MarkdownHtmlTablesFixture.Caption &&
            !captionedGroup.IsKeyboardFocusable() && captionedGroup.GetPattern(PatternInterface.Invoke) == null,
            "The visible caption names the table group, maps to exact caption text, and creates no interactive tab stop.");
        var tableBounds = captionedGroup.GetBoundingRectangle();
        var labelBounds = captionPeer.GetBoundingRectangle();
        Check(tableBounds.Width >= labelBounds.Width && tableBounds.Height > labelBounds.Height && labelBounds.Height > 0,
            "The named table group covers the caption and grid geometry.");
        ITextRangeProvider Find(string value)
        {
            var range = text.DocumentRange.FindText(value, false, false);
            Check(range != null && range.GetText(-1) == value, "The actual HTML table exposes native text: " + value);
            return range;
        }
        var identity = preview.GetSelection();
        log.AppendLine($"OBS: Markdown.HtmlTables initial selection identity={identity.DocumentId}/{identity.Revision}/{identity.TextGeneration}, " +
            $"range={identity.Start}+{identity.Length}, available={identity.IsAvailable}, followSource={extension.FollowSource}, " +
            $"sourceCaret={native.Anchor}/{native.CurrentPos}; offset-zero alias is checked by explicit SelectRange next.");
        void CheckAlias(ulong from, ulong target)
        {
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, target, 0);
            var expected = text.GetSelection().Single();
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, from, 0);
            var selected = preview.GetSelection();
            var actual = text.GetSelection().Single();
            actual.GetBoundingRectangles(out var bounds);
            Check(selected.Start == target && selected.Length == 0 && actual.Compare(expected) && bounds is not { Length: > 0 },
                $"Covered/caption point {from} resolves to owner boundary {target} with empty UI Automation bounds.");
        }
        CheckAlias(0, 1);
        var captionEnd = (ulong)(preview.DisplayedText.IndexOf(MarkdownHtmlTablesFixture.Caption, StringComparison.Ordinal) +
            MarkdownHtmlTablesFixture.Caption.Length);
        var spanStart = (ulong)preview.DisplayedText.IndexOf(MarkdownHtmlTablesFixture.SpanText, StringComparison.Ordinal);
        var spanEnd = spanStart + (ulong)MarkdownHtmlTablesFixture.SpanText.Length;
        CheckAlias(captionEnd + 1, captionEnd);
        CheckAlias(spanEnd + 1, spanEnd);
        void CheckSelected(ulong start, ulong length, string expected, string label)
        {
            var selected = preview.GetSelection();
            var actual = text.GetSelection().Single().GetText(-1);
            Check(selected.Start == start && selected.Length == length && actual == expected,
                $"{label}: exact nonempty TSV range {start}+{length}; actual={selected.Start}+{selected.Length}, " +
                $"text={actual.Replace("\t", "\\t").Replace("\n", "\\n")}.");
        }
        void SelectText(ulong start, string expected)
        {
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, start, (ulong)expected.Length);
            CheckSelected(start, (ulong)expected.Length, expected, "SelectRange");
        }
        SelectText(0, "\n");
        SelectText(captionEnd, "\t");
        SelectText(spanEnd, "\t");
        SelectText(spanStart, MarkdownHtmlTablesFixture.SpanText + "\t");
        SelectText(0, MarkdownHtmlTablesFixture.Main);
        var originalWidth = preview.Width;
        var narrowWidth = Math.Max(100, preview.ActualWidth * 0.8);
        async Task RelayoutAsync(double width)
        {
            var layout = preview.GetViewportAnchor().LayoutGeneration;
            preview.Width = width;
            await UntilAsync(() => extension.SourceSession.IsCurrent && preview.IsSurfaceReady &&
                preview.GetViewportAnchor().IsAvailable && preview.GetViewportAnchor().LayoutGeneration > layout,
                "The actual table width change publishes a new frame for selection reinstallation.");
        }
        async Task RelayoutSelectedAsync(double width, ulong start, string expected)
        {
            await RelayoutAsync(width);
            CheckSelected(start, (ulong)expected.Length, expected, "Native frame reinstallation after width change");
        }
        try
        {
            await RelayoutSelectedAsync(narrowWidth, 0, MarkdownHtmlTablesFixture.Main);
            Find("\n").Select();
            CheckSelected(0, 1, "\n", "UIA LF-only Select");
            await RelayoutSelectedAsync(originalWidth, 0, "\n");
            Find("\t").Select();
            CheckSelected(captionEnd, 1, "\t", "UIA tab-only Select");
            await RelayoutSelectedAsync(narrowWidth, captionEnd, "\t");
            Find(MarkdownHtmlTablesFixture.SpanText + "\t").Select();
            CheckSelected(spanStart, (ulong)MarkdownHtmlTablesFixture.SpanText.Length + 1,
                MarkdownHtmlTablesFixture.SpanText + "\t", "UIA suffix-tab Select");
            await RelayoutSelectedAsync(originalWidth, spanStart, MarkdownHtmlTablesFixture.SpanText + "\t");
        }
        finally
        {
            if (!preview.Width.Equals(originalWidth))
                await RelayoutAsync(originalWidth);
        }
        CheckSelected(spanStart, (ulong)MarkdownHtmlTablesFixture.SpanText.Length + 1,
            MarkdownHtmlTablesFixture.SpanText + "\t", "Restored-width frame reinstallation");
        Find("列一").GetBoundingRectangles(out var firstHeader);
        Find("列二").GetBoundingRectangles(out var secondHeader);
        Find(MarkdownHtmlTablesFixture.Caption).GetBoundingRectangles(out var captionBounds);
        Find("R2C1").GetBoundingRectangles(out var firstBody);
        Find("x < y").GetBoundingRectangles(out var codeBounds);
        Check(firstHeader.Length == 4 && secondHeader.Length == 4 && captionBounds.Length == 4 &&
            firstBody.Length == 4 && codeBounds.Length == 4 &&
            Math.Abs(firstHeader[1] - secondHeader[1]) < 1 && secondHeader[0] > firstHeader[0] &&
            captionBounds[1] < firstHeader[1] && Math.Abs(firstBody[1] - codeBounds[1]) < 1,
            "The caption precedes the two real header columns and the final td/code cells share one native row.");
        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, spanStart,
            (ulong)MarkdownHtmlTablesFixture.SpanText.Length);
        Check(text.GetSelection().Single().GetText(-1) == MarkdownHtmlTablesFixture.SpanText,
            "Selecting spanning-cell text retains its one logical owner without duplicated covered-cell text.");
        var plainStart = (ulong)preview.DisplayedText.IndexOf("R2C1", StringComparison.Ordinal);
        preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, plainStart, 4);
        CheckSelected(plainStart, 4, "R2C1", "Exact plain-cell Main/UIA word selection");
        var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart, plainStart + 4, MarkdownPositionAffinity.After);
        var expectedByte = Encoding.UTF8.GetByteCount(source[..source.IndexOf("R2C1", StringComparison.Ordinal)]);
        var mappedBefore = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart, plainStart + 4, MarkdownPositionAffinity.Before);
        var startAfter = await preview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart, MarkdownPositionAffinity.After);
        var endAfter = await preview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart + 4, MarkdownPositionAffinity.After);
        var startBefore = await preview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart, MarkdownPositionAffinity.Before);
        var endBefore = await preview.MapPreviewPositionAsync(identity.DocumentId, identity.Revision,
            identity.TextGeneration, plainStart + 4, MarkdownPositionAffinity.Before);
        static string Escaped(string value) => string.Concat(value.Select(unit => $"\\u{(int)unit:X4}"));
        static string DescribePoint(MarkdownMappedPosition value) =>
            $"identity={value.DocumentId}/{value.Revision}/{value.TextGeneration}, sourceByte={value.SourceByteOffset}, " +
            $"mainUtf16={value.PreviewUtf16Offset}, affinity={value.Affinity}, correspondence={value.Correspondence}, effects=0x{value.Effects:X8}";
        static bool SamePoint(MarkdownMappedPosition left, MarkdownMappedPosition right) =>
            left.DocumentId == right.DocumentId && left.Revision == right.Revision && left.TextGeneration == right.TextGeneration &&
            left.SourceByteOffset == right.SourceByteOffset && left.PreviewUtf16Offset == right.PreviewUtf16Offset &&
            left.Affinity == right.Affinity && left.Correspondence == right.Correspondence && left.Effects == right.Effects;
        static string Slice(byte[] snapshot, ulong start, ulong end)
        {
            if (start > end || end > (ulong)snapshot.Length) return $"invalid[{start},{end}) for length={snapshot.Length}";
            var length = (int)Math.Min(128UL, end - start);
            return $"[{start},{end}) byteLength={end - start}, captured={length}, " +
                $"utf8='{Escaped(Encoding.UTF8.GetString(snapshot, (int)start, length))}', " +
                $"hex={Convert.ToHexString(snapshot.AsSpan((int)start, length))}";
        }
        static string Window(byte[] snapshot, ulong point)
        {
            if (point > (ulong)snapshot.Length) return $"point={point} out of length={snapshot.Length}";
            return Slice(snapshot, point > 24 ? point - 24 : 0, Math.Min((ulong)snapshot.Length, point + 48));
        }
        var currentBytesAtMap = ReadCurrentBytes(core);
        var currentSourceAtMap = editor.GetText();
        var cellSource = "<td>R2C1</td>";
        var cellSourceStart = source.IndexOf(cellSource, StringComparison.Ordinal);
        Check(cellSourceStart >= 0, "The frozen source independently contains the authored R2C1 td envelope for diagnostic observation.");
        var cellByteStart = Encoding.UTF8.GetByteCount(source[..cellSourceStart]);
        var cellByteEnd = cellByteStart + Encoding.UTF8.GetByteCount(cellSource);
        log.AppendLine($"OBS: Markdown.HtmlTables mapping request identity={identity.DocumentId}/{identity.Revision}/{identity.TextGeneration}, " +
            $"mainUtf16=[{plainStart},{plainStart + 4}), word='{Escaped(preview.DisplayedText.Substring((int)plainStart, 4))}', " +
            $"expectedCurrentUtf8=[{expectedByte},{expectedByte + 4}), expectedDiskUtf8={Encoding.UTF8.GetByteCount(MarkdownHtmlTablesFixture.DiskSource[..MarkdownHtmlTablesFixture.DiskSource.IndexOf("R2C1", StringComparison.Ordinal)])}.");
        log.AppendLine("OBS: Markdown.HtmlTables span.After.start " + DescribePoint(mapped.Start));
        log.AppendLine("OBS: Markdown.HtmlTables span.After.end " + DescribePoint(mapped.End));
        log.AppendLine("OBS: Markdown.HtmlTables span.Before.start " + DescribePoint(mappedBefore.Start));
        log.AppendLine("OBS: Markdown.HtmlTables span.Before.end " + DescribePoint(mappedBefore.End));
        log.AppendLine("OBS: Markdown.HtmlTables point.start.After " + DescribePoint(startAfter));
        log.AppendLine("OBS: Markdown.HtmlTables point.end.After " + DescribePoint(endAfter));
        log.AppendLine("OBS: Markdown.HtmlTables point.start.Before " + DescribePoint(startBefore));
        log.AppendLine("OBS: Markdown.HtmlTables point.end.Before " + DescribePoint(endBefore));
        log.AppendLine($"OBS: Markdown.HtmlTables sameAffinityContract After={SamePoint(mapped.Start, startAfter) && SamePoint(mapped.End, endAfter)}, " +
            $"Before={SamePoint(mappedBefore.Start, startBefore) && SamePoint(mappedBefore.End, endBefore)}; " +
            $"mixedAfterBeforeExact={startAfter.SourceByteOffset == (ulong)expectedByte && endBefore.SourceByteOffset == (ulong)expectedByte + 4}.");
        log.AppendLine("OBS: Markdown.HtmlTables frozen expected slice " + Slice(bytes, (ulong)expectedByte, (ulong)expectedByte + 4));
        log.AppendLine("OBS: Markdown.HtmlTables frozen span.After slice " + Slice(bytes, mapped.Start.SourceByteOffset, mapped.End.SourceByteOffset));
        log.AppendLine("OBS: Markdown.HtmlTables frozen span.Before slice " + Slice(bytes, mappedBefore.Start.SourceByteOffset, mappedBefore.End.SourceByteOffset));
        log.AppendLine("OBS: Markdown.HtmlTables frozen mixedAfterBefore slice " + Slice(bytes, startAfter.SourceByteOffset, endBefore.SourceByteOffset));
        log.AppendLine("OBS: Markdown.HtmlTables independently authored owner envelope " + Slice(bytes, (ulong)cellByteStart, (ulong)cellByteEnd));
        log.AppendLine($"OBS: Markdown.HtmlTables owner envelope observations [{cellByteStart},{cellByteEnd}), " +
            $"startBeforeInside={startBefore.SourceByteOffset >= (ulong)cellByteStart && startBefore.SourceByteOffset <= (ulong)cellByteEnd}, " +
            $"startAfterInside={startAfter.SourceByteOffset >= (ulong)cellByteStart && startAfter.SourceByteOffset <= (ulong)cellByteEnd}, " +
            $"endBeforeInside={endBefore.SourceByteOffset >= (ulong)cellByteStart && endBefore.SourceByteOffset <= (ulong)cellByteEnd}, " +
            $"endAfterInside={endAfter.SourceByteOffset >= (ulong)cellByteStart && endAfter.SourceByteOffset <= (ulong)cellByteEnd}.");
        log.AppendLine("OBS: Markdown.HtmlTables start.After source window " + Window(bytes, mapped.Start.SourceByteOffset));
        log.AppendLine("OBS: Markdown.HtmlTables end.After source window " + Window(bytes, mapped.End.SourceByteOffset));
        log.AppendLine($"OBS: Markdown.HtmlTables mapping source state currentUtf8Length={currentBytesAtMap.Length}, " +
            $"sha256={Convert.ToHexString(SHA256.HashData(currentBytesAtMap))}, CR={currentSourceAtMap.Count(unit => unit == '\r')}, " +
            $"LF={currentSourceAtMap.Count(unit => unit == '\n')}, bytesFrozen={currentBytesAtMap.SequenceEqual(bytes)}, " +
            $"textFrozen={currentSourceAtMap == source}, undo={core.CanUndo}/{originalUndo}, modified={editor.IsModified}/{originalModified}, " +
            $"sourceSelection={native.Anchor}/{native.CurrentPos}, expectedSelection={sourceSelection.Anchor}/{sourceSelection.CurrentPos}.");
        // MapReadingSpan maps two independent points with the same requested
        // affinity. It does not choose the inward affinities of a text range.
        Check(SamePoint(mapped.Start, startAfter) && SamePoint(mapped.End, endAfter) &&
            SamePoint(mappedBefore.Start, startBefore) && SamePoint(mappedBefore.End, endBefore) &&
            mapped.Start.SourceByteOffset <= mapped.End.SourceByteOffset && mapped.End.SourceByteOffset <= (ulong)bytes.Length &&
            mappedBefore.Start.SourceByteOffset <= mappedBefore.End.SourceByteOffset && mappedBefore.End.SourceByteOffset <= (ulong)bytes.Length,
            $"Both public span affinities retain their independent same-affinity endpoint contract and valid source ranges; " +
            $"After=({DescribePoint(mapped.Start)})..({DescribePoint(mapped.End)}), " +
            $"Before=({DescribePoint(mappedBefore.Start)})..({DescribePoint(mappedBefore.End)}).");
        Check(startAfter.SourceByteOffset == (ulong)expectedByte && endBefore.SourceByteOffset == (ulong)expectedByte + 4 &&
            startAfter.PreviewUtf16Offset == plainStart && endBefore.PreviewUtf16Offset == plainStart + 4 &&
            startAfter.DocumentId == identity.DocumentId && startAfter.Revision == identity.Revision && startAfter.TextGeneration == identity.TextGeneration &&
            endBefore.DocumentId == identity.DocumentId && endBefore.Revision == identity.Revision && endBefore.TextGeneration == identity.TextGeneration &&
            Encoding.UTF8.GetString(bytes, expectedByte, 4) == "R2C1",
            $"The inward start-After/end-Before body endpoints map to the independently authored exact current UTF8 word, " +
            $"actual=[{startAfter.SourceByteOffset},{endBefore.SourceByteOffset}), expected=[{expectedByte},{expectedByte + 4}), " +
            $"start=({DescribePoint(startAfter)}), end=({DescribePoint(endBefore)}), " +
            $"slice={Slice(bytes, startAfter.SourceByteOffset, endBefore.SourceByteOffset)}.");
        Check(ReadCurrentBytes(core).SequenceEqual(bytes) && editor.GetText() == source && core.CanUndo == originalUndo &&
            editor.IsModified == originalModified && sourceSelection == (native.Anchor, native.CurrentPos),
            "Table UIA selection/mapping preserves canonical source bytes, source selection, Undo and save point.");

        // A long authored colspan makes the actual glyph row exceed one header
        // column. This tests product geometry without exposing private C++ rows.
        native.SetSel(0, core.DocumentLength);
        editor.TypeText(MarkdownHtmlTablesFixture.LongSource);
        await CurrentAsync(editor);
        Check(preview.DisplayedText == MarkdownHtmlTablesFixture.LongMain, "A real source edit publishes the authored long colspan Main.");
        identity = preview.GetSelection();
        preview.TrySetVerticalScrollProgress(0);
        await TabDragTestHost.FrameAsync();
        Find("列一").GetBoundingRectangles(out firstHeader);
        Find("列二").GetBoundingRectangles(out secondHeader);
        Find(MarkdownHtmlTablesFixture.LongSpanText).GetBoundingRectangles(out var longBounds);
        Check(firstHeader.Length == 4 && secondHeader.Length == 4 && longBounds.Length >= 4,
            "Actual long-span and header geometry are available after publication.");
        var columnAdvance = secondHeader[0] - firstHeader[0];
        var longest = Enumerable.Range(0, longBounds.Length / 4).Max(index => longBounds[index * 4 + 2]);
        Check(columnAdvance > 0 && longest > columnAdvance,
            $"True colspan text uses more than one real column: max glyph row={longest:R}, header column advance={columnAdvance:R}.");
        log.AppendLine($"OBS: Markdown.HtmlTables colspan maxGlyphWidth={longest:R}, columnAdvance={columnAdvance:R}, fragments={longBounds.Length / 4}.");
        core.Undo();
        await CurrentAsync(editor);
        Check(preview.DisplayedText == MarkdownHtmlTablesFixture.Main && ReadCurrentBytes(core).SequenceEqual(bytes) &&
            editor.GetText() == source && core.CanUndo == originalUndo && editor.IsModified == originalModified,
            "Actual native Undo restores the exact original table model, canonical bytes and save point.");
        await BytesAsync(editor.EditingFile, diskBytes, "Table editing/Undo leaves the authored CRLF disk file unchanged.");

        var rich = await OpenAsync(host, "html-table-rich.md", Encoding.UTF8.GetBytes(MarkdownHtmlTablesFixture.RichSource));
        rich.ShowHideContentPreview();
        var richExtension = await CurrentAsync(rich);
        var richPeer = FrameworkElementAutomationPeer.FromElement(richExtension.PreviewView) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(richExtension.PreviewView);
        Check(richExtension.PreviewView.DisplayedText == MarkdownHtmlTablesFixture.RichMain &&
            ((ITextProvider)richPeer.GetPattern(PatternInterface.Text)).DocumentRange.GetText(-1) == MarkdownHtmlTablesFixture.RichMain,
            "Mixed td/th without thead preserves plain Main and one atomic image position.");
        var rootChildren = richPeer.GetChildren();
        var richGroup = rootChildren.Single(child => child.GetAutomationControlType() == AutomationControlType.Group &&
            child.GetName() == MarkdownHtmlTablesFixture.RichCaption);
        var children = richGroup.GetChildren();
        Check(children.Count(child => child.GetAutomationControlType() == AutomationControlType.Hyperlink && child.GetName() == "native link") == 1 &&
            children.Count(child => child.GetAutomationControlType() == AutomationControlType.Image &&
                child.GetName().StartsWith("Table alt", StringComparison.Ordinal)) == 1 &&
            rootChildren.All(child => child.GetAutomationControlType() is not (AutomationControlType.Hyperlink or AutomationControlType.Image)),
            "Links and images retain one semantic peer under their captioned group.");

        var richText = (ITextProvider)richPeer.GetPattern(PatternInterface.Text);
        var documentChildren = richText.DocumentRange.GetChildren();
        Check(documentChildren?.Length == 1, "The one-table document range exposes its captioned group.");
        var groupRange = richText.RangeFromChild(documentChildren.Single());
        var rawChildren = groupRange?.GetChildren();
        Check(rawChildren?.Length == 3, "The group range contains its caption and the two cell peers.");
        var rangeCaption = richText.RangeFromChild(rawChildren.First());
        var captionChildren = rangeCaption?.GetChildren();
        Check(captionChildren?.Length == 1 && rangeCaption.GetText(-1) == MarkdownHtmlTablesFixture.RichCaption &&
            richGroup.GetLabeledBy().GetChildren().Single().GetName() == "and values",
            "The caption range and peer both contain the caption's single link.");
        var rangeLink = richText.RangeFromChild(captionChildren.Single());
        Check(rangeLink != null && rangeLink.GetChildren() is not { Length: > 0 } &&
            richText.RangeFromChild(rangeLink.GetEnclosingElement())?.GetText(-1) == "and values" &&
            richText.RangeFromChild(rangeCaption.GetEnclosingElement())?.GetText(-1) == MarkdownHtmlTablesFixture.RichCaption &&
            richText.RangeFromChild(groupRange.GetEnclosingElement())?.GetText(-1) == groupRange.GetText(-1),
            "The leaf link has no child, and the enclosing text ranges identify the link, caption and group.");
        log.AppendLine("PASS: Markdown.HtmlTableCaption — exact label association, native group geometry and matching UIA tree and text-range ancestry.");
        Check(rich.GetText() == MarkdownHtmlTablesFixture.RichSource &&
            ReadCurrentBytes(rich.TestNativeCore).SequenceEqual(Encoding.UTF8.GetBytes(MarkdownHtmlTablesFixture.RichSource)),
            "Table inline semantics do not mutate the source document.");
        const string spans = "<table><tbody><tr><td rowspan=0 colspan=2>跨行列😀</td><td>第一行</td></tr>" +
            "<tr><td>第二行</td></tr></tbody><tbody><tr><td colspan=3>下一组</td></tr></tbody></table>";
        const string spanMain = "跨行列😀\t\t第一行\n\t\t第二行\n下一组\t\t";
        var mergedEditor = await OpenAsync(host, "html-table-rowspan.md", Encoding.UTF8.GetBytes(spans));
        mergedEditor.ShowHideContentPreview();
        var mergedView = (await CurrentAsync(mergedEditor)).PreviewView;
        var mergedPeer = FrameworkElementAutomationPeer.FromElement(mergedView) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(mergedView);
        var mergedText = (ITextProvider)mergedPeer.GetPattern(PatternInterface.Text);
        Check(mergedView.DisplayedText == spanMain && mergedText.DocumentRange.GetText(-1) == spanMain &&
            mergedEditor.GetText() == spans, "Rowspan 0 ends at its authored row group and exposes each owner only once.");
        var firstRow = mergedText.DocumentRange.FindText("第一行", false, false);
        var secondRow = mergedText.DocumentRange.FindText("第二行", false, false);
        firstRow.GetBoundingRectangles(out var firstPosition);
        secondRow.GetBoundingRectangles(out var secondPosition);
        Check(firstPosition.Length == 4 && secondPosition.Length == 4 &&
            Math.Abs(firstPosition[0] - secondPosition[0]) < 1 && secondPosition[1] > firstPosition[1],
            "The second row skips both vertically covered columns and keeps its third-column geometry.");
        var mergedIdentity = mergedView.GetSelection();
        var coveredStart = (ulong)spanMain.IndexOf("\t\t第二行", StringComparison.Ordinal);
        mergedView.SelectRange(mergedIdentity.DocumentId, mergedIdentity.Revision, mergedIdentity.TextGeneration, coveredStart, 0);
        var mergedCaret = mergedView.GetSelection();
        Check(mergedCaret.Start == coveredStart + 2 && mergedCaret.Length == 0,
            "A caret in vertically covered slots goes directly to the next visible cell.");
        mergedText.DocumentRange.FindText("\t\t第二行", false, false).Select();
        Check(mergedText.GetSelection().Single().GetText(-1) == "\t\t第二行",
            "Selecting covered row slots preserves their exact TSV separators.");
        log.AppendLine("PASS: Markdown.HtmlTableRowspans — combined spans, tbody-scoped zero, exact UIA TSV and aligned uncovered column.");
        const string foldedSource = "<details>\r<summary>Captioned body</summary>\r\r" +
            "<table><caption>Hidden costs</caption><tr><td>1</td></tr></table>\r\r</details>\r\r" +
            "<table><caption>Visible costs</caption><tr><td>2</td></tr></table>";
        var folded = await OpenAsync(host, "html-table-folded-caption.md", Encoding.UTF8.GetBytes(foldedSource));
        folded.ShowHideContentPreview();
        var foldedView = (await CurrentAsync(folded)).PreviewView;
        var foldedPeer = FrameworkElementAutomationPeer.FromElement(foldedView) ??
            FrameworkElementAutomationPeer.CreatePeerForElement(foldedView);
        AutomationPeer Group(string name) => foldedPeer.GetChildren()?.SingleOrDefault(child =>
            child.GetAutomationControlType() == AutomationControlType.Group && child.GetName() == name);
        IExpandCollapseProvider Fold() => foldedPeer.GetChildren()?.Single(child => child.GetName() == "Captioned body")
            .GetPattern(PatternInterface.ExpandCollapse) as IExpandCollapseProvider;
        static bool IsUnavailable(AutomationPeer provider)
        {
            try
            {
                _ = provider.GetName();
                return false;
            }
            catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201)
            {
                return true;
            }
        }

        Check(Group("Hidden costs") == null && Group("Visible costs")?.GetLabeledBy()?.GetName() == "Visible costs",
            "Only the visible captioned group is present before a disclosure opens.");
        Fold().Expand();
        await UntilAsync(() => foldedView.GetViewportAnchor().IsAvailable && Group("Hidden costs") != null,
            "The captioned group excluded from initial layout appears after disclosure expansion.");
        var retainedGroup = Group("Hidden costs");
        var retainedCaption = retainedGroup.GetLabeledBy();
        Check(retainedCaption.GetName() == "Hidden costs" && Group("Visible costs").GetLabeledBy().GetName() == "Visible costs",
            "Two captioned tables retain independent label identities.");
        Fold().Collapse();
        await UntilAsync(() => foldedView.GetViewportAnchor().IsAvailable && Group("Hidden costs") == null,
            "A collapsed table group is removed from the current automation tree.");
        Check(IsUnavailable(retainedGroup) && IsUnavailable(retainedCaption),
            "A retained group and label reject live calls while the table is hidden.");
        Fold().Expand();
        await UntilAsync(() => foldedView.GetViewportAnchor().IsAvailable && Group("Hidden costs") != null,
            "The original table owner is visible again.");
        Check(retainedCaption.GetName() == "Hidden costs",
            "A disclosure relayout keeps the caption peer usable with the same snapshot.");
        var captionByte = Encoding.UTF8.GetByteCount(foldedSource[..foldedSource.IndexOf("Hidden costs", StringComparison.Ordinal)]);
        folded.TestNativeCore.TestFeatureNative.SetSel(captionByte, captionByte + "Hidden costs".Length);
        folded.TypeText("New costs");
        await CurrentAsync(folded);
        Check(IsUnavailable(retainedGroup) && IsUnavailable(retainedCaption),
            "Accepted source replacement retires both the old table group and caption peer.");
        log.AppendLine("PASS: Markdown.HtmlTableCaptionLifetime — two titles, collapse and re-expansion, and old peer retirement.");

        log.AppendLine("PASS: Markdown.HtmlTables — current canonical CR versus unchanged disk CRLF; actual caption/thead/tbody/code and rectangular TSV; explicit offset-zero empty-caption/covered caret aliases with source-follow disabled before publication; exact nonempty LF/tab-only/suffix-tab public and UIA selections across width-frame reinstallation; same-affinity public span versus point contracts and independently exact inward cell-body UTF8 endpoints; real long-colspan width greater than one column; source edit/Undo/save point; mixed td/th and one link/image peer. Native17 including malformed wire/cache/cancel/pixels remains an independent H4 diagnostic; content-fit and browser clipboard bytes are not claimed.");
    }
}
