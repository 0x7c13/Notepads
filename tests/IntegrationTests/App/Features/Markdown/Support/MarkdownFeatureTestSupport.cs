// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsWorkspaceTests;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Markup;
using Windows.UI.Xaml.Media;
using Windows.UI.ViewManagement;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    internal static async Task<MarkdownExtensionView> CurrentAsync(TextEditor editor)
    {
        await UntilAsync(() =>
        {
            var extension = editor.TestMarkdownPreview;
            var source = extension?.SourceSession;
            var view = extension?.PreviewView;
            if (source == null || view == null || !source.IsCurrent || source.Status != MarkdownSessionStatus.Ready) return false;
            var accepted = view.GetSourceState();
            return accepted.StateKnown && accepted.HasSource && (!accepted.NeedsResync || view.HasCachedDocument) &&
                accepted.DocumentId == source.DocumentId && accepted.Revision == source.AcceptedRevision &&
                view.DisplayedDocumentId == source.DocumentId && view.DisplayedRevision == source.AcceptedRevision &&
                view.IsSurfaceReady && string.IsNullOrEmpty(view.RenderingError) &&
                HasReadingIdentity(view, source.DocumentId, source.AcceptedRevision);
        }, "The actual Markdown source, editor stamp and displayed native frame must all name the current revision.", 25000);
        return editor.TestMarkdownPreview;
    }

    private static async Task CompareFullSyncAsync(FeatureTestHost host, TextEditor editor, string label, StringBuilder log)
    {
        var current = await CurrentAsync(editor);
        var expected = current.PreviewView.DisplayedText;
        var bytes = ReadCurrentBytes(editor.TestNativeCore);
        var root = host.Page.Content as Panel;
        Check(root != null, "The real MainPage root can host the independently submitted full-sync oracle.");
        var oracle = new MarkdownView
        {
            Width = 420, Height = 240, IsHitTestVisible = false,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top
        };
        root.Children.Add(oracle);
        try
        {
            await UntilAsync(() => oracle.IsLoaded && oracle.ActualWidth > 0, "The independent full-sync Markdown control is loaded.");
            var document = Guid.NewGuid();
            var result = await oracle.SetDocumentWithExtensionsAsync(document, 1, bytes,
                MarkdownExtensions.Math | MarkdownExtensions.Mermaid |
                MarkdownExtensions.HtmlBlocks | MarkdownExtensions.HtmlInline);
            Check(result.Status == DocumentUpdateStatus.Ready && result.Published, "Independent complete-source submission publishes: " + label);
            Check(oracle.DisplayedText == expected, "Integrated incremental text equals an independent full-source render: " + label);
            try
            {
                await UntilAsync(() => HasReadingIdentity(oracle, document, 1) &&
                    HasReadingIdentity(current.PreviewView, current.SourceSession.DocumentId, current.SourceSession.AcceptedRevision),
                    "Both full-sync controls expose current reading identities before outline comparison: " + label, 25000);
            }
            catch (TimeoutException)
            {
                LogViewportDiagnostics(log, oracle, "full-sync oracle reading timed out: " + label, includeParts: true);
                LogViewportDiagnostics(log, current.PreviewView, "full-sync integrated reading timed out: " + label, includeParts: true);
                throw;
            }
            var expectedOutline = oracle.GetOutline();
            log.AppendLine($"OBS: Markdown.FullSync oracle outline count={expectedOutline.Count}: {label}.");
            var actualOutline = current.PreviewView.GetOutline();
            log.AppendLine($"OBS: Markdown.FullSync integrated outline count={actualOutline.Count}: {label}.");
            Check(expectedOutline.Count == actualOutline.Count,
                "Integrated and full-source heading counts agree: " + label);
        }
        finally
        {
            oracle.Cancel();
            root.Children.Remove(oracle);
        }
    }

    private static bool HasReadingIdentity(MarkdownView view, Guid document, ulong revision)
    {
        var selection = view.GetSelection();
        var anchor = view.GetViewportAnchor();
        return view.IsSurfaceReady && string.IsNullOrEmpty(view.RenderingError) &&
            selection.IsAvailable && anchor.IsAvailable &&
            selection.DocumentId == document && selection.Revision == revision &&
            anchor.DocumentId == document && anchor.Revision == revision &&
            anchor.TextGeneration == selection.TextGeneration;
    }

    private static byte[] ReadCurrentBytes(TextEditorCore core)
    {
        using var bytes = new MemoryStream();
        var length = core.DocumentLength;
        for (long offset = 0; offset < length;)
        {
            var chunk = core.ReadCurrentUtf8Range(offset, Math.Max(4, checked((int)Math.Min(65536, length - offset))));
            Check(chunk.Length != 0, "The native full-sync oracle read must make scalar-aligned progress.");
            var owned = new byte[chunk.Length];
            using (var reader = DataReader.FromBuffer(chunk)) reader.ReadBytes(owned);
            bytes.Write(owned, 0, owned.Length);
            offset += owned.Length;
        }
        return bytes.ToArray();
    }

    private static bool ImageState(MarkdownView preview, string alt, bool loaded)
    {
        var peer = FrameworkElementAutomationPeer.FromElement(preview) ?? FrameworkElementAutomationPeer.CreatePeerForElement(preview);
        var state = alt + (loaded ? ", image loaded" : ", image not loaded");
        try
        {
            return peer?.GetChildren()?.Any(child => child.GetAutomationControlType() == AutomationControlType.Image &&
                child.GetName().StartsWith(state, StringComparison.Ordinal)) == true;
        }
        catch (Exception error) when (unchecked((uint)error.HResult) == 0x80040201)
        {
            return false;
        }
    }
}
