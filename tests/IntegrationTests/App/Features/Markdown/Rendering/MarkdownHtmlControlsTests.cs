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
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class MarkdownHtmlControlsFixture
{
    internal const string DiskSource = "# Static HTML checkbox\r\n\r\n" +
        "<label><input type=\"CHECK&#98;OX\" checked disabled>中😀 <b>Alpha</b> &amp; one</label>\r\n\r\n" +
        "<label><input type=\"checkbox\" disabled=\"false\">Beta</label>\r\n\r\n" +
        "<label><input type=\"checkbox\" checked=\"false\" disabled>Gamma</label>\r\n\r\n" +
        "- [ ] GFM\r\n\r\n" +
        "<label><input type=\"checkbox\" disabled>Image label <img src=\"missing-h1.png\" alt=\"图标\"></label>\r\n\r\n" +
        "<label><input type=\"checkbox\">Enabled fallback</label>\r\n\r\n" +
        "<label><input type=\"radio\" disabled>Radio fallback</label>\r\n\r\n" +
        "<label for=\"external\"><input type=\"checkbox\" disabled>Associated fallback</label>\r\n\r\n" +
        "<label><input type=\"checkbox\" disabled><input type=\"checkbox\" disabled>Multiple fallback</label>";
    internal static readonly string Source = DiskSource.Replace("\r\n", "\r");
    internal static readonly string[] Labels = ["中😀 Alpha & one", "Beta", "Gamma"];
}

internal static partial class MarkdownFeatureTests
{
    internal static async Task RunHtmlControlsAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var diskSource = MarkdownHtmlControlsFixture.DiskSource;
        var diskBytes = Encoding.UTF8.GetBytes(diskSource);
        var expectedSource = MarkdownHtmlControlsFixture.Source;
        var expectedBytes = Encoding.UTF8.GetBytes(expectedSource);
        var editor = await OpenAsync(host, "html-controls.md", diskBytes);
        var core = editor.TestNativeCore;
        var source = editor.GetText();
        var bytes = ReadCurrentBytes(core);
        Check(source == expectedSource && bytes.SequenceEqual(expectedBytes),
            "Opening the CRLF disk fixture produces the independent canonical CR editor text and exact current UTF-8 bytes.");
        await BytesAsync(editor.EditingFile, diskBytes, "Opening the fixture preserves its authored CRLF disk bytes.");
        static string SourceEvidence(string value, byte[] utf8) =>
            $"utf8Length={utf8.Length}, sha256={Convert.ToHexString(SHA256.HashData(utf8))}, " +
            $"CR={value.Count(unit => unit == '\r')}, LF={value.Count(unit => unit == '\n')}, " +
            $"CRLF={(value.Length - value.Replace("\r\n", "").Length) / 2}";
        log.AppendLine("OBS: Markdown.HtmlControls disk-to-editor canonicalization: disk " +
            SourceEvidence(diskSource, diskBytes) + "; frozen current editor " + SourceEvidence(source, bytes) + ".");
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var preview = extension.PreviewView;
        var originalInteraction = preview.TaskInteractionEnabled;
        var originalUndo = core.CanUndo;
        var originalModified = editor.IsModified;
        var sourceSelection = (core.TestFeatureNative.Anchor, core.TestFeatureNative.CurrentPos);
        var requests = 0;
        MarkdownTaskToggleRequestedEventArgs lastRequest = null;
        void Requested(MarkdownView sender, MarkdownTaskToggleRequestedEventArgs args)
        {
            requests++;
            lastRequest = args;
        }
        preview.TaskToggleRequested += Requested;
        try
        {
            preview.TaskInteractionEnabled = true;
            var peer = FrameworkElementAutomationPeer.FromElement(preview) ??
                FrameworkElementAutomationPeer.CreatePeerForElement(preview);
            var text = (ITextProvider)peer.GetPattern(PatternInterface.Text);
            var identity = preview.GetSelection();
            var children = peer.GetChildren();
            Check(children.Count(child => child.GetAutomationControlType() == AutomationControlType.CheckBox) == 5,
                "Exactly four static HTML checkboxes and one enabled original GFM task have native checkbox peers.");
            for (var index = 0; index < MarkdownHtmlControlsFixture.Labels.Length; index++)
            {
                var label = MarkdownHtmlControlsFixture.Labels[index];
                var checkbox = children.Single(child => child.GetName() == label);
                Check(checkbox.GetAutomationControlType() == AutomationControlType.CheckBox &&
                    !checkbox.IsEnabled() && !checkbox.IsKeyboardFocusable(),
                    "The static HTML checkbox exposes its full decoded label and disabled native UIA semantics: " + label);
                var toggle = checkbox.GetPattern(PatternInterface.Toggle) as IToggleProvider;
                var expected = index == 1 ? ToggleState.Off : ToggleState.On;
                Check(toggle != null && toggle.ToggleState == expected,
                    "Disabled HTML state is readable; checked/disabled boolean false spellings retain presence semantics.");
                var rejected = false;
                try { toggle.Toggle(); }
                catch (Exception error) when (error.HResult == unchecked((int)0x80040200)) { rejected = true; }
                Check(rejected && requests == 0 && toggle.ToggleState == expected,
                    "A real disabled UIA Toggle rejects with ELEMENTNOTENABLED and emits no task change request.");
                var bounds = checkbox.GetBoundingRectangle();
                Check(bounds.Width > 0 && bounds.Height > 0, "The actual disabled checkbox has painted native bounds.");
                var labelRange = text.DocumentRange.FindText(label, false, false);
                Check(labelRange != null && labelRange.GetText(-1) == label,
                    "The static label remains readable and selectable as ordinary native text.");
                var start = (ulong)preview.DisplayedText.IndexOf(label, StringComparison.Ordinal);
                preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, start, (ulong)label.Length);
                Check(text.GetSelection().Single().GetText(-1) == label,
                    "Selecting the full HTML label preserves decoded CJK/emoji and inline formatting text.");
            }
            var imageLabel = children.Single(child => child.GetName() == "Image label 图标");
            Check(!imageLabel.IsEnabled() && imageLabel.GetAutomationControlType() == AutomationControlType.CheckBox &&
                ((IToggleProvider)imageLabel.GetPattern(PatternInterface.Toggle)).ToggleState == ToggleState.Off,
                "An allowed safe inline image contributes its complete alt text to the disabled checkbox name.");
            var main = preview.DisplayedText;
            Check(!main.Contains("<b>Alpha</b>", StringComparison.Ordinal) &&
                main.Contains("<input type=\"radio\" disabled>", StringComparison.Ordinal) &&
                main.Contains("<input type=\"checkbox\">Enabled fallback", StringComparison.Ordinal) &&
                main.Contains("for=\"external\"", StringComparison.Ordinal) &&
                main.Contains("Multiple fallback", StringComparison.Ordinal),
                "Safe supported labels render, while enabled/other-type/associated/multiple-control fragments remain inert.");
            var objectStart = (ulong)main.IndexOf('\xfffc');
            preview.SelectRange(identity.DocumentId, identity.Revision, identity.TextGeneration, objectStart, 1);
            var selectedObject = text.GetSelection().Single().GetText(-1);
            var escapedObject = string.Concat(selectedObject.Select(value => $"\\u{(int)value:X4}"));
            log.AppendLine($"OBS: Markdown.HtmlControls atomic Main selection length={selectedObject.Length}; escaped='{escapedObject}'.");
            Check(selectedObject == "\ufffc",
                $"UIA object selection retains one atomic Main U+FFFC; actual length={selectedObject.Length}, escaped='{escapedObject}'.");
            var mapped = await preview.MapPreviewSpanAsync(identity.DocumentId, identity.Revision,
                identity.TextGeneration, objectStart, objectStart + 1, MarkdownPositionAffinity.After);
            var mappedSource = Encoding.UTF8.GetString(bytes, (int)mapped.Start.SourceByteOffset,
                (int)(mapped.End.SourceByteOffset - mapped.Start.SourceByteOffset));
            Check(mappedSource.Contains("input", StringComparison.OrdinalIgnoreCase) &&
                mappedSource.Contains("checked", StringComparison.OrdinalIgnoreCase),
                "The synthetic checkbox maps back to its original HTML input envelope.");

            var gfm = peer.GetChildren().Single(child => child.GetName() == "Unchecked task");
            Check(gfm.IsEnabled() && gfm.IsKeyboardFocusable(), "Enabling tasks keeps the ordinary GFM interaction path.");
            var requestsBeforeGfm = requests;
            ((IToggleProvider)gfm.GetPattern(PatternInterface.Toggle)).Toggle();
            var expectedChar = source.IndexOf("[ ] GFM", StringComparison.Ordinal);
            var expectedByte = Encoding.UTF8.GetByteCount(source[..expectedChar]);
            static string Escaped(string value) => value == null ? "<null>" :
                string.Concat(value.Select(unit => $"\\u{(int)unit:X4}"));
            var expectedSlice = Encoding.UTF8.GetString(bytes, expectedByte, 3);
            var requestActual = "lastRequest=null";
            if (lastRequest != null)
            {
                var offset = lastRequest.SourceByteOffset;
                var length = lastRequest.SourceByteLength;
                var actualSlice = offset <= (ulong)bytes.Length && length <= (ulong)bytes.Length - offset
                    ? Encoding.UTF8.GetString(bytes, (int)offset, (int)length) : "<outside-original-source>";
                requestActual = $"lastRequest document={lastRequest.DocumentId}, revision={lastRequest.Revision}, " +
                    $"textGeneration={lastRequest.TextGeneration}, requestId={lastRequest.RequestId}, taskId={lastRequest.TaskId}, " +
                    $"MainStart={lastRequest.Start}, MainLength={lastRequest.Length}, " +
                    $"SourceByteOffset={offset}, SourceByteLength={length}, " +
                    $"ExpectedSourceEscaped='{Escaped(lastRequest.ExpectedSource)}', " +
                    $"IsChecked={lastRequest.IsChecked}, RequestedChecked={lastRequest.RequestedChecked}, " +
                    $"originalSourceSliceEscaped='{Escaped(actualSlice)}'";
            }
            var requestExpected = $"expectedUTF8Byte={expectedByte}, expectedUTF16Char={expectedChar}, " +
                $"expectedSourceSliceEscaped='{Escaped(expectedSlice)}', expectedByteLength=3, " +
                "expectedIsChecked=False, expectedRequestedChecked=True";
            var requestObservation = $"requestsBefore={requestsBeforeGfm}, requestsAfter={requests}; " +
                requestActual + "; " + requestExpected;
            log.AppendLine("OBS: Markdown.HtmlControls synchronous GFM request: " + requestObservation + ".");
            Check(requests == 1 && lastRequest != null && lastRequest.SourceByteOffset == (ulong)expectedByte &&
                lastRequest.SourceByteLength == 3 && lastRequest.ExpectedSource == "[ ]" &&
                !lastRequest.IsChecked && lastRequest.RequestedChecked,
                "The original GFM task emits its exact UTF-8 three-byte host edit request. Actual: " + requestObservation + ".");
            preview.TaskInteractionEnabled = false;
            await UntilAsync(() => preview.GetSelection() is { IsAvailable: true } current &&
                current.DocumentId == identity.DocumentId && current.Revision == identity.Revision &&
                current.TextGeneration == identity.TextGeneration,
                "The original reading snapshot is available after changing task interaction.");
            Check(gfm.GetAutomationControlType() == AutomationControlType.Text &&
                gfm.GetPattern(PatternInterface.Toggle) == null &&
                peer.GetChildren().Single(child => child.GetName() == MarkdownHtmlControlsFixture.Labels[0])
                    .GetAutomationControlType() == AutomationControlType.CheckBox,
                "Ordinary read-only GFM keeps its existing Text semantics while static HTML remains a disabled checkbox.");
            Check(ReadCurrentBytes(core).SequenceEqual(bytes) && editor.GetText() == source &&
                core.CanUndo == originalUndo && editor.IsModified == originalModified &&
                sourceSelection == (core.TestFeatureNative.Anchor, core.TestFeatureNative.CurrentPos),
                "Checkbox UIA actions and preview selections preserve original UTF-8, source selection, Undo and save point.");

            // Publishing a new source retires the old checkbox peer rather than
            // allowing its captured source identity to become an edit capability.
            var stale = (IToggleProvider)children.Single(child => child.GetName() == MarkdownHtmlControlsFixture.Labels[0])
                .GetPattern(PatternInterface.Toggle);
            core.TestFeatureNative.SetSel(0, core.DocumentLength);
            editor.TypeText("# Replacement\r\n\r\nNo HTML control.");
            await CurrentAsync(editor);
            var staleRejected = false;
            try { _ = stale.ToggleState; }
            catch (Exception error) when (error.HResult == unchecked((int)0x80040201)) { staleRejected = true; }
            Check(staleRejected && requests == 1, "A retired source snapshot rejects the old HTML checkbox peer.");
            core.Undo();
            await CurrentAsync(editor);
            Check(ReadCurrentBytes(core).SequenceEqual(bytes) && editor.GetText() == source &&
                preview.DisplayedText.Contains("中😀 Alpha & one", StringComparison.Ordinal),
                "Real native Undo restores frozen canonical HTML text/bytes and reconstructs the disabled checkbox model.");
            await BytesAsync(editor.EditingFile, diskBytes, "Preview actions and actual source Undo preserve the authored CRLF disk bytes.");
            log.AppendLine("PASS: Markdown.HtmlControls — CRLF disk fixture opens as verified canonical CR current UTF-8 with unchanged disk bytes; actual disabled CheckBox UIA, full labels/readable state/Toggle rejection; boolean/type/entity/CJK/emoji; inert unsupported controls; atomic Main U+FFFC selection/current-source mapping; ordinary GFM exact current UTF-8 request and unchanged read-only semantics; canonical source bytes/selection/Undo/save point; retired peer and actual source Undo. Semantic state-marker copy is a separate Native diagnostic; physical pointer/keyboard remain the external H1 fixture gate.");
        }
        finally
        {
            preview.TaskToggleRequested -= Requested;
            preview.TaskInteractionEnabled = originalInteraction;
        }
    }
}
