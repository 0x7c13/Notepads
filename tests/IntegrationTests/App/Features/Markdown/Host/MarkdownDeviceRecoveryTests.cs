// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using System.Threading.Tasks;
using CommunityToolkit.WinUI;
using Notepads.Markdown;
using Notepads.Presentation.Controls.Markdown;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static partial class MarkdownFeatureTests
{
    private static async Task InitialViewportDeviceRecoveryAsync(FeatureTestHost host, StringBuilder log)
    {
        const ulong initialViewportDeviceLossPhase = 25;
        var document = "# Initial device recovery\r\r" +
            string.Concat(Enumerable.Repeat("A paragraph to fill the speculative preview viewport.\r\r", 200));
        var editor = await OpenAsync(host, "initial-device-recovery.md", Encoding.UTF8.GetBytes(document));
        CoreValidationProbe.StartPreviewStartupTrace(false);
        ulong[] phases;
        try
        {
            editor.ShowHideContentPreview();
            CoreValidationProbe.ArmPreviewDeviceLoss(editor.TestMarkdownPreview.PreviewView, true);
            var extension = await CurrentAsync(editor);
            var session = extension.SourceSession;
            var receipt = session.TestLastSubmissionReceipt;
            Check(receipt is { Status: DocumentUpdateStatus.Ready, Published: true } &&
                receipt.DocumentId == session.DocumentId && receipt.Revision == session.AcceptedRevision &&
                session.FullSubmissions == 1 && session.DeltaSubmissions == 0 && editor.GetText() == document,
                "The first full host update publishes after a device fault without a second submission or reopening.");
        }
        finally { phases = CoreValidationProbe.StopPreviewStartupTrace(); }
        Check(Enumerable.Range(0, phases.Length / 3).Count(index =>
            phases[index * 3] == initialViewportDeviceLossPhase) == 1,
            "The device loss was actually injected into the speculative prefix, before the receipt was claimed.");
        log.AppendLine("PASS: Markdown.InitialViewportDeviceLoss recovers the same pending host receipt after a prefix graphics fault.");
    }

    internal static async Task RunDeviceRecoveryAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        await InitialViewportDeviceRecoveryAsync(host, log);
        var editor = await OpenAsync(host, "device-recovery.md", Encoding.UTF8.GetBytes("# Device recovery\r\rExact host source Ω😀."));
        editor.ShowHideContentPreview();
        var extension = await CurrentAsync(editor);
        extension.FollowSource = false;
        var session = extension.SourceSession;
        var preview = extension.PreviewView;
        var source = editor.GetText();
        var full = session.FullSubmissions;
        var delta = session.DeltaSubmissions;
        DocumentUpdateResult failed = null;
        MarkdownEditorStamp failedStamp = default;
        void ObserveFailure(object sender, EventArgs args)
        {
            if (session.Status != MarkdownSessionStatus.Failed || failed != null) return;
            failed = session.TestLastSubmissionReceipt;
            failedStamp = session.AcceptedStamp;
        }
        session.Changed += ObserveFailure;
        try
        {
            CoreValidationProbe.ArmPreviewDeviceLoss(preview, false);
            session.RequestFullSync();
            await UntilAsync(() => failed != null, "A real claimed full-publication device fault returns its failed first receipt.");
            await CurrentAsync(editor);
            Check(failed.Status == DocumentUpdateStatus.Failed && !failed.Published &&
                failed.DocumentId == session.DocumentId && failed.Revision == session.AcceptedRevision &&
                session.AcceptedStamp == failedStamp && session.FullSubmissions == full + 1 && session.DeltaSubmissions == delta,
                "The asynchronous native repaint recovers the exact accepted host submission without rewriting its failed receipt or resubmitting source.");
            var banner = extension.FindDescendant("PreviewStatus") as TextBlock;
            Check(editor.GetText() == source && banner?.Visibility == Visibility.Collapsed,
                "Native device recovery preserves source and clears the host failure banner.");
        }
        finally { session.Changed -= ObserveFailure; }

        // Reentrant typing is newer than the failed admission, even if native
        // graphics recovery subsequently paints that older accepted revision.
        failed = null;
        bool staleReady = false;
        void EditAfterFailure(object sender, EventArgs args)
        {
            if (session.Status == MarkdownSessionStatus.Ready && failed != null && session.AcceptedRevision == failed.Revision)
                staleReady = true;
            if (session.Status != MarkdownSessionStatus.Failed || failed != null) return;
            failed = session.TestLastSubmissionReceipt;
            var native = editor.TestNativeCore.TestFeatureNative;
            native.SetSel(native.Length, native.Length);
            editor.TypeText(" newer source");
        }
        session.Changed += EditAfterFailure;
        try
        {
            full = session.FullSubmissions;
            CoreValidationProbe.ArmPreviewDeviceLoss(preview, false);
            session.RequestFullSync();
            await UntilAsync(() => failed != null, "The dirty-source recovery case reaches its actual failed full receipt.");
            await CurrentAsync(editor);
            Check(!staleReady && session.AcceptedRevision > failed.Revision && session.FullSubmissions == full + 2 &&
                editor.GetText() == source + " newer source" && preview.DisplayedText.Contains("newer source", StringComparison.Ordinal),
                "A newer edit requires its own full admission and cannot be revived by the failed revision's later repaint.");
        }
        finally { session.Changed -= EditAfterFailure; }

        // A foreign native document may paint successfully, but it cannot become
        // the source authority of this host session or clear its failed status.
        failed = null;
        Task<DocumentUpdateResult> foreignWork = null;
        var foreign = Guid.NewGuid();
        void ReplaceNativeAfterFailure(object sender, EventArgs args)
        {
            if (session.Status != MarkdownSessionStatus.Failed || failed != null) return;
            failed = session.TestLastSubmissionReceipt;
            failedStamp = session.AcceptedStamp;
            foreignWork = preview.SetDocumentWithExtensionsAsync(foreign, 1,
                Encoding.UTF8.GetBytes("# Foreign native source"), session.Extensions).AsTask();
        }
        session.Changed += ReplaceNativeAfterFailure;
        try
        {
            CoreValidationProbe.ArmPreviewDeviceLoss(preview, false);
            session.RequestFullSync();
            await UntilAsync(() => foreignWork != null, "The foreign-source recovery case reaches its failed host receipt.");
            var foreignResult = await foreignWork.WaitAsync(TimeSpan.FromSeconds(20));
            Check(foreignResult.Status == DocumentUpdateStatus.Ready && foreignResult.Published &&
                preview.DisplayedDocumentId == foreign && session.Status == MarkdownSessionStatus.Failed &&
                !session.IsCurrent && session.AcceptedRevision == failed.Revision && session.AcceptedStamp == failedStamp,
                "A healthy foreign native snapshot cannot adopt or revive the failed host identity.");
        }
        finally { session.Changed -= ReplaceNativeAfterFailure; }
        session.RequestFullSync();
        await CurrentAsync(editor);

        // Stopping the host leaves native recovery free to finish, while the
        // retired host generation remains disabled until its explicit Start.
        failed = null;
        void StopAfterFailure(object sender, EventArgs args)
        {
            if (session.Status != MarkdownSessionStatus.Failed || failed != null) return;
            failed = session.TestLastSubmissionReceipt;
            failedStamp = session.AcceptedStamp;
            session.Stop(preserveDocument: true);
        }
        session.Changed += StopAfterFailure;
        try
        {
            CoreValidationProbe.ArmPreviewDeviceLoss(preview, false);
            session.RequestFullSync();
            await UntilAsync(() => failed != null && preview.IsSurfaceReady && preview.DisplayedDocumentId == session.DocumentId &&
                preview.DisplayedRevision == failed.Revision && preview.GetViewportAnchor().IsAvailable,
                "The actual native device retry finishes after the host session has stopped.");
            Check(session.Status == MarkdownSessionStatus.Disabled && !session.IsCurrent &&
                session.AcceptedRevision == failed.Revision && session.AcceptedStamp == failedStamp,
                "A later native repaint cannot reactivate a retired host generation.");
        }
        finally { session.Changed -= StopAfterFailure; }
        session.Start();
        await CurrentAsync(editor);

        var empty = await OpenAsync(host, "device-recovery-thematic.md", Encoding.UTF8.GetBytes("***\r"));
        empty.ShowHideContentPreview();
        var emptyExtension = empty.TestMarkdownPreview;
        var emptySession = emptyExtension.SourceSession;
        var emptyPreview = emptyExtension.PreviewView;
        emptyExtension.FollowSource = false;
        bool EmptyCurrent() => emptySession.IsCurrent && emptySession.Status == MarkdownSessionStatus.Ready &&
            emptyPreview.IsSurfaceReady && emptyPreview.DisplayedDocumentId == emptySession.DocumentId &&
            emptyPreview.DisplayedRevision == emptySession.AcceptedRevision;
        await UntilAsync(EmptyCurrent, "The thematic-only host document publishes without requiring a text anchor.");
        Check(!emptyPreview.GetViewportAnchor().IsAvailable, "The recovery counterexample has no text anchor.");
        DocumentUpdateResult emptyFailed = null;
        void ObserveEmptyFailure(object sender, EventArgs args)
        {
            if (emptySession.Status == MarkdownSessionStatus.Failed)
                emptyFailed ??= emptySession.TestLastSubmissionReceipt;
        }
        emptySession.Changed += ObserveEmptyFailure;
        try
        {
            full = emptySession.FullSubmissions;
            CoreValidationProbe.ArmPreviewDeviceLoss(emptyPreview, false);
            emptySession.RequestFullSync();
            await UntilAsync(() => emptyFailed != null && EmptyCurrent(),
                "A recovered surface notification revives the exact thematic-only admission without a text anchor.");
            Check(emptyFailed.Status == DocumentUpdateStatus.Failed && !emptyFailed.Published &&
                emptySession.AcceptedRevision == emptyFailed.Revision && emptySession.FullSubmissions == full + 1 &&
                !emptyPreview.GetViewportAnchor().IsAvailable && empty.GetText() == "***\r",
                "Anchorless recovery preserves the failed first receipt, accepted identity, source and single submission.");
        }
        finally { emptySession.Changed -= ObserveEmptyFailure; }
        log.AppendLine("PASS: Markdown.DeviceRecovery exercises real one-shot device-loss receipts and native async repaint, exact host/banner and anchorless thematic recovery, newer typing, foreign native source and stopped-generation rejection.");
    }
}
