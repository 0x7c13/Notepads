// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.DocumentAssets;
using Notepads.Infrastructure.Threading;
using Notepads.Markdown;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    private const int MaximumNoteDepth = 3;
    private const int MaximumMountedNotes = 8;
    private const long MaximumRetainedNoteBytes = 12L * 1024 * 1024;
    private readonly Dictionary<uint, MountedNote> _notes = new();
    private readonly Dictionary<MarkdownView, MountedNote> _noteHosts = new();
    private MarkdownTheme _noteTheme;
    private int _noteAdmissions;
    private long _retainedNoteBytes;
    private bool _noteRefillScheduled;
    private ulong _noteCacheClock;

    // For a brokered note, Grant stays the original StorageFile and RelativePath identifies the child.
    private readonly record struct NoteOrigin(StorageFile Grant, string RelativePath = null)
    {
        public string DocumentPath
        {
            get
            {
                var path = Grant?.Path;
                return string.IsNullOrEmpty(path) || RelativePath == null ? path :
                    Path.Combine(Path.GetDirectoryName(path), RelativePath);
            }
        }
    }

    private sealed class MountedNote(MarkdownView owner, MountedNote parent, Guid document, ulong revision,
        ulong textGeneration, MarkdownNoteEmbedSlot slot)
    {
        public MarkdownView Owner { get; } = owner;
        public MountedNote Parent { get; } = parent;
        public int Depth { get; } = (parent?.Depth ?? 0) + 1;
        public Dictionary<uint, MountedNote> Children { get; } = new();
        public Guid Document { get; } = document;
        public ulong Revision { get; set; } = revision;
        public ulong TextGeneration { get; set; } = textGeneration;
        public MarkdownNoteEmbedSlot Slot { get; set; } = slot;
        public ulong? DraftRevision { get; set; }
        public bool Retired { get; set; }
        public ulong? OffscreenSince { get; set; }
        public CancellationTokenSource Lifetime { get; } = new();
        public Task Work { get; set; } = Task.CompletedTask;
        public Task DraftWork { get; set; } = Task.CompletedTask;
        public MarkdownView Child { get; set; }
        public MarkdownImageProvider Images { get; set; }
        public NoteOrigin Origin { get; set; }
        public (MarkdownLinkRequestedEventArgs Args, ulong Request)? PendingLink { get; set; }
        public ulong? LinkRequest { get; set; }
        public long DeferredSourceBytes { get; set; }
        public string Heading { get; set; }
        public bool Attempted { get; set; }
        public ulong? RejectedMountLayout { get; set; }
        public bool Admitted { get; set; }
        public long SourceBytes { get; set; }
    }

    private void RefreshNoteEmbeds()
    {
        if (!_active || _disposed || _view.Visibility != Visibility.Visible)
        {
            RetireNoteEmbeds();
            return;
        }
        if (!_session.IsCurrent)
        {
            if (_session.Status is MarkdownSessionStatus.Updating or MarkdownSessionStatus.Restoring)
                RetireSupersededNoteLoads(_notes);
            else
                RetireNoteEmbeds();
            return;
        }
        RefreshNoteEmbeds(_view, new NoteOrigin(_owner.EditingFile), null);
    }

    private void RefreshNoteEmbeds(MarkdownView owner, NoteOrigin origin, MountedNote parent)
    {
        if (parent?.Depth >= MaximumNoteDepth || !IsNoteBranchVisible(parent)) return;
        var notes = parent?.Children ?? _notes;
        var identity = owner.GetSelection();
        if (!identity.IsAvailable) return;
        var slots = owner.GetVisibleNoteEmbeds();
        bool CurrentBinding(MountedNote note) => note.Document == identity.DocumentId &&
            note.Revision == identity.Revision && note.TextGeneration == identity.TextGeneration &&
            (note.DraftRevision == null || note.DraftRevision == _session.AcceptedRevision);
        var previous = notes.Values.ToArray();
        // Remove obsolete keys before adopting the placements from the new
        // frame. Inserting an earlier object can shift every subsequent ID.
        foreach (var note in previous)
            if (!CurrentBinding(note)) notes.Remove(note.Slot.ObjectId);
        foreach (var note in previous)
            if (!CurrentBinding(note)) TryAdoptNoteEmbed(note, identity, slots, notes);
        // Decide and unlink the entire retirement set before detaching any
        // child. A detach can reenter and adopt a survivor into a newer frame.
        var retiring = new List<MountedNote>();
        foreach (var note in previous)
        {
            if (!CurrentBinding(note))
                retiring.Add(note);
            else if (slots.Any(slot => slot.ObjectId == note.Slot.ObjectId))
                note.OffscreenSince = null;
            else if (note.Child is { IsLoaded: true } && note.Work.IsCompletedSuccessfully)
            {
                if (!note.OffscreenSince.HasValue)
                {
                    note.OffscreenSince = ++_noteCacheClock;
                    ClearNoteBranchLinks(note);
                }
            }
            else
                retiring.Add(note);
        }
        foreach (var note in retiring) UntrackNoteEmbed(note);
        foreach (var note in retiring) RetireNoteEmbed(note);
        if (!_active || _disposed || !_session.IsCurrent || (parent != null && !IsCurrentNote(parent))) return;
        var current = owner.GetSelection();
        if (!current.IsAvailable || current.DocumentId != identity.DocumentId || current.Revision != identity.Revision ||
            current.TextGeneration != identity.TextGeneration) return;
        foreach (var slot in retiring.Count == 0 ? slots : owner.GetVisibleNoteEmbeds())
        {
            if (!notes.TryGetValue(slot.ObjectId, out var note))
            {
                note = new MountedNote(owner, parent, identity.DocumentId, identity.Revision, identity.TextGeneration, slot);
                notes.Add(slot.ObjectId, note);
            }
            note.OffscreenSince = null;
            if (note.Attempted || note.Admitted ||
                (note.RejectedMountLayout is ulong rejected && rejected == owner.GetViewportAnchor().LayoutGeneration))
                continue;
            // Eviction detaches XAML children, which can reenter refresh and
            // admit this same slot before the outer call resumes.
            if (!MakeRoomForNote(note.DeferredSourceBytes, needsAdmission: true) ||
                !IsNoteBranchVisible(note) || !IsCurrentNote(note) || note.Attempted || note.Admitted)
                continue;
            note.DeferredSourceBytes = 0;
            note.Attempted = note.Admitted = true;
            ++_noteAdmissions;
            note.Work = LoadNoteEmbedAsync(note, origin);
        }

        // A descendant can finish reading during an ancestor's fit reflow.
        // Retry the mounted branch when that ancestor has a usable frame,
        // even if its capped height did not change the root layout.
        foreach (var note in notes.Values.ToArray())
            if (note.Child != null && IsCurrentNote(note)) RefreshNoteEmbeds(note.Child, note.Origin, note);
    }

    private void TryAdoptNoteEmbed(MountedNote note, MarkdownSelection identity,
        IEnumerable<MarkdownNoteEmbedSlot> slots, Dictionary<uint, MountedNote> notes)
    {
        if (note.Retired || !note.Work.IsCompletedSuccessfully ||
            (note.DraftRevision is ulong draft && draft != _session.AcceptedRevision) ||
            note.Child is not { IsLoaded: true } child || note.Document != identity.DocumentId) return;
        foreach (var slot in slots)
        {
            if (notes.ContainsKey(slot.ObjectId) || slot.Target != note.Slot.Target ||
                !note.Owner.SetNoteEmbedChild(identity.DocumentId, identity.Revision, identity.TextGeneration,
                    slot.ObjectId, child)) continue;
            note.Revision = identity.Revision;
            note.TextGeneration = identity.TextGeneration;
            note.Slot = slot;
            notes.Add(slot.ObjectId, note);
            return;
        }
    }

    private void RetireSupersededNoteLoads(Dictionary<uint, MountedNote> notes)
    {
        foreach (var note in notes.Values.ToArray())
        {
            if (note.Child == null || !note.Work.IsCompletedSuccessfully)
                RetireNoteEmbed(note);
            else
                RetireSupersededNoteLoads(note.Children);
        }
    }

    private void ForwardDraftNoteEdit(MarkdownSourceSession.AcceptedEdit edit)
    {
        foreach (var note in _noteHosts.Values.ToArray())
        {
            if (!_active || _disposed || _session.AcceptedRevision != edit.Revision) return;
            if (note.DraftRevision == null || note.Retired) continue;
            if (!IsNoteBranchVisible(note) || note.DraftRevision != edit.BaseRevision || !note.Work.IsCompletedSuccessfully ||
                note.Child is not { IsLoaded: true } child ||
                edit.Length - note.SourceBytes > MaximumRetainedNoteBytes - _retainedNoteBytes)
            {
                RetireNoteEmbed(note);
                continue;
            }
            // Reentrant retirement must drain this edit and any earlier receipt.
            var complete = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            note.DraftWork = note.DraftWork.IsCompletedSuccessfully ? complete.Task : Task.WhenAll(note.DraftWork, complete.Task);
            _ = ApplyDraftNoteEditAsync(note, child, edit, complete);
        }
    }

    private async Task ApplyDraftNoteEditAsync(MountedNote note, MarkdownView child, MarkdownSourceSession.AcceptedEdit edit,
        TaskCompletionSource<bool> complete)
    {
        var cancellation = note.Lifetime.Token;
        var accepted = false;
        var observed = false;
        var ready = false;
        Task<DocumentUpdateResult> operation = null;
        try
        {
            var prior = child.GetSourceState();
            if (prior.StateKnown && prior.HasSource && !prior.NeedsResync)
            {
                var revision = checked(prior.Revision + 1);
                operation = child.ApplyEditsAsync(prior.DocumentId, prior.Revision, revision, [edit.Edit], edit.Bytes)
                    .AsCompletionTask(cancellation);
                var source = child.GetSourceState();
                var growth = edit.Length - note.SourceBytes;
                if (!note.Retired && source.StateKnown && source.HasSource && !source.NeedsResync &&
                    source.DocumentId == prior.DocumentId && source.Revision == revision &&
                    growth <= MaximumRetainedNoteBytes - _retainedNoteBytes)
                {
                    _retainedNoteBytes += growth;
                    note.SourceBytes = edit.Length;
                    note.DraftRevision = edit.Revision;
                    accepted = true;
                }
                observed = true;
                var result = await operation;
                ready = accepted && result.Status == DocumentUpdateStatus.Ready && result.Published;
            }
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        catch (Exception error) { LoggingService.LogException(error); }
        finally
        {
            try
            {
                // Observe even an already-faulted receipt if acceptance
                // inspection threw before its normal await.
                if (operation != null && !observed)
                {
                    try { await operation; }
                    catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
                    catch (Exception error) { LoggingService.LogException(error); }
                }
                var ownsRevision = note.DraftRevision == (accepted ? edit.Revision : edit.BaseRevision);
                if (!ready && !note.Retired && ownsRevision)
                    RetireNoteEmbed(note);
            }
            catch (Exception error) { LoggingService.LogException(error); }
            finally { complete.TrySetResult(true); }
        }
    }

    private bool IsCurrentNote(MountedNote note)
    {
        var siblings = note.Parent?.Children ?? _notes;
        if (!_active || _disposed || !_session.IsCurrent || note.Retired || note.Lifetime.IsCancellationRequested ||
            (note.DraftRevision is ulong draftRevision && draftRevision != _session.AcceptedRevision) ||
            !siblings.TryGetValue(note.Slot.ObjectId, out var retained) || !ReferenceEquals(retained, note) ||
            (note.Parent != null && !IsCurrentNote(note.Parent)))
            return false;
        var current = note.Owner.GetSelection();
        return current.IsAvailable && current.DocumentId == note.Document && current.Revision == note.Revision &&
            current.TextGeneration == note.TextGeneration;
    }

    private bool CanContinueNoteLoad(MountedNote note)
    {
        if (IsCurrentNote(note)) return true;
        note.Attempted = false;
        return false;
    }

    private async Task LoadNoteEmbedAsync(MountedNote note, NoteOrigin origin)
    {
        var cancellation = note.Lifetime.Token;
        try
        {
            var target = note.Slot.Target;
            var hash = target.IndexOf('#');
            var path = (hash < 0 ? target : target[..hash]).Trim();
            var heading = hash < 0 ? null : target[(hash + 1)..].Trim();
            if (heading?.Length == 0) heading = null;
            var targetOrigin = origin;
            if (path.Length != 0)
            {
                if (origin.Grant == null || GetRelativeNotePath(origin.RelativePath, path) is not string relative)
                    return;
                targetOrigin = new NoteOrigin(origin.Grant, relative);
            }
            if (!CanContinueNoteLoad(note) || IsNoteCycle(note, origin, targetOrigin, heading)) return;

            byte[] source;
            if (SameNoteFile(targetOrigin, new NoteOrigin(_owner.EditingFile)))
            {
                if (!ReserveNoteBytes(note, _session.AcceptedStamp.Length)) return;
                var draftRevision = _session.AcceptedRevision;
                note.DraftRevision = draftRevision;
                source = await _session.ReadAcceptedNoteSourceAsync(draftRevision, cancellation);
            }
            else
            {
                var (resolved, decoded) = await DecodeNoteAsync(targetOrigin, cancellation);
                if (decoded == null) return;
                try
                {
                    if (!CanContinueNoteLoad(note) ||
                        (resolved != targetOrigin && IsNoteCycle(note, origin, resolved, heading)) ||
                        !ReserveNoteBytes(note, decoded.Baseline.ByteLength))
                        return;
                    targetOrigin = resolved;
                    using var stream = await decoded.Baseline.OpenReadStreamAsync(cancellation);
                    source = new byte[checked((int)note.SourceBytes)];
                    await stream.ReadExactlyAsync(source, cancellation);
                }
                finally { await decoded.DisposeAsync(); }
            }
            if (!CanContinueNoteLoad(note) || source == null) return;

            var child = new MarkdownView();
            if (_noteTheme != null) child.SetTheme(_noteTheme);
            if (!note.Owner.SetNoteEmbedChild(note.Document, note.Revision, note.TextGeneration, note.Slot.ObjectId, child))
            {
                // A different layout may provide enough room to mount the child.
                note.RejectedMountLayout = note.Owner.GetViewportAnchor().LayoutGeneration;
                note.Attempted = false;
                return;
            }
            note.Child = child;
            note.Origin = targetOrigin;
            note.Heading = heading;
            _noteHosts.Add(child, note);
            if (heading != null) child.RestrictNoteEmbedToHeading(heading);
            note.Images = new MarkdownImageProvider(targetOrigin.Grant, targetOrigin.RelativePath);
            child.SetImageProvider(note.Images, 1);
            child.LinkRequested += NoteEmbedLinkRequested;
            child.CodeCopied += PreviewCodeCopied;
            child.ViewportChanged += NoteEmbedViewportChanged;
            child.VerticalScrollChanged += NoteEmbedScrollChanged;
            child.VerticalScrollStarting += NoteEmbedScrollStarting;
            child.Unloaded += NoteEmbedUnloaded;
            var result = await child.SetDocumentWithExtensionsAsync(Guid.NewGuid(), 1, source,
                _session.AcceptedExtensions).AsCompletionTask(cancellation);
            if (!IsCurrentNote(note)) return;
            if (result.Status == DocumentUpdateStatus.Ready)
                RefreshNoteEmbeds(child, targetOrigin, note);
            else
                DetachNoteEmbed(note);
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        catch (Exception error) when (note.Child == null && error is
            (FileNotFoundException or DirectoryNotFoundException or UnauthorizedAccessException or
             ArgumentException or NotSupportedException or PathTooLongException)) { }
        catch (Exception error)
        {
            if (IsCurrentNote(note)) DetachNoteEmbed(note);
            LoggingService.LogException(error);
        }
        finally
        {
            if (note.Child == null) ReleaseNoteAdmission(note);
        }
    }

    private static string GetRelativeNotePath(string originPath, string target)
    {
        if (Path.IsPathRooted(target) || target.Contains(':')) return null;
        var parts = target.Replace('/', '\\').Split('\\');
        if (parts[^1] == "." ||
            parts.Any(part => part != "." && (part.Length == 0 || part.EndsWith('.') || part.EndsWith(' '))))
            return null;
        var relative = string.Join('\\', parts.Where(part => part != "."));
        if (DocumentLanguages.Detect(relative, default).Id != "markdown") relative += ".md";
        return originPath == null ? relative : Path.Combine(Path.GetDirectoryName(originPath), relative);
    }

    private static async Task<(NoteOrigin Origin, DocumentSnapshot Snapshot)> DecodeNoteAsync(
        NoteOrigin target, CancellationToken cancellation)
    {
        var options = new DocumentLoadOptions(decodingMode: DocumentDecodingMode.AutoDetect);
        const long maximumBytes = MarkdownSourceSession.MaximumSourceBytes;
        if (target.RelativePath is string relative)
        {
            if (!string.IsNullOrEmpty(target.Grant.Path))
            {
                using var asset = await DocumentAssetClient.ReadDocumentAssetAsync(
                    target.Grant, relative, (ulong)maximumBytes, cancellation);
                if (asset != null)
                {
                    using var stream = asset.AsStream();
                    var decoded = await DocumentTextPipeline.DecodeSnapshotAsync(stream, options, Guid.NewGuid(),
                        cancellationToken: cancellation, maximumInputBytes: maximumBytes);
                    return (target, decoded);
                }
            }
            var file = await ResolveNoteFileAsync(target.Grant, relative, cancellation);
            if (file == null) return default;
            target = new NoteOrigin(file);
        }

        return (target, await DocumentTextPipeline.DecodeFileAsync(target.Grant, options, Guid.NewGuid(),
            cancellationToken: cancellation, maximumInputBytes: maximumBytes));
    }

    private static async Task<StorageFile> ResolveNoteFileAsync(StorageFile origin, string relative, CancellationToken cancellation)
    {
        try
        {
            var parent = await origin.GetParentAsync().AsCompletionTask(cancellation);
            if (parent != null) return await parent.GetFileAsync(relative).AsCompletionTask(cancellation);
        }
        catch (UnauthorizedAccessException)
        {
            // The app can read package-local paths even when no parent grant was supplied.
            cancellation.ThrowIfCancellationRequested();
        }
        var directory = string.IsNullOrEmpty(origin.Path) ? null : Path.GetDirectoryName(origin.Path);
        return directory == null ? null :
            await StorageFile.GetFileFromPathAsync(Path.Combine(directory, relative)).AsCompletionTask(cancellation);
    }

    private bool IsNoteCycle(MountedNote note, NoteOrigin origin, NoteOrigin file, string heading)
    {
        var isLocalSection = heading != null && SameNoteFile(file, origin);
        if (SameNoteFile(file, new NoteOrigin(_owner.EditingFile)) && !isLocalSection) return true;
        for (var ancestor = note.Parent; ancestor != null; ancestor = ancestor.Parent)
        {
            if (!SameNoteFile(file, ancestor.Origin)) continue;
            if (!isLocalSection || string.Equals(heading, ancestor.Heading, StringComparison.OrdinalIgnoreCase))
                return true;
        }
        return false;
    }

    private static bool SameNoteFile(NoteOrigin left, NoteOrigin right)
    {
        if (left.Grant == null || right.Grant == null) return left.Grant == null && right.Grant == null;
        if (left.RelativePath == null && right.RelativePath == null) return left.Grant.IsEqual(right.Grant);
        if (left.Grant.IsEqual(right.Grant) &&
            string.Equals(left.RelativePath, right.RelativePath, StringComparison.OrdinalIgnoreCase))
            return true;
        var path = left.DocumentPath;
        return !string.IsNullOrEmpty(path) && string.Equals(path, right.DocumentPath, StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsNoteBranchVisible(MountedNote note)
    {
        for (; note != null; note = note.Parent)
            if (note.OffscreenSince.HasValue) return false;
        return true;
    }

    private bool MakeRoomForNote(long bytes, bool needsAdmission)
    {
        if (bytes > MaximumRetainedNoteBytes) return false;
        while ((needsAdmission && _noteAdmissions >= MaximumMountedNotes) ||
               bytes > MaximumRetainedNoteBytes - _retainedNoteBytes)
        {
            // The native overlay hides offscreen mounts. Retain their frame and
            // descendants until a visible request needs one of the shared limits.
            var oldest = _noteHosts.Values
                .Where(note => note.OffscreenSince.HasValue && IsNoteBranchVisible(note.Parent))
                .MinBy(note => note.OffscreenSince.Value);
            if (oldest == null) return false;
            RetireNoteEmbed(oldest);
        }
        return true;
    }

    private bool ReserveNoteBytes(MountedNote note, long bytes)
    {
        if (bytes < 0 || bytes > MarkdownSourceSession.MaximumSourceBytes) return false;
        if (!MakeRoomForNote(bytes, needsAdmission: false))
        {
            note.DeferredSourceBytes = bytes;
            note.Attempted = false;
            return false;
        }
        if (!CanContinueNoteLoad(note)) return false;
        _retainedNoteBytes += bytes;
        note.SourceBytes = bytes;
        return true;
    }

    private void ReleaseNoteAdmission(MountedNote note)
    {
        if (!note.Admitted) return;
        note.Admitted = false;
        _retainedNoteBytes -= note.SourceBytes;
        note.SourceBytes = 0;
        --_noteAdmissions;
        if (!_active || _disposed || _noteRefillScheduled) return;
        _noteRefillScheduled = true;
        _ = Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
        {
            _noteRefillScheduled = false;
            RefreshNoteEmbeds();
        });
    }

    private void RetireNoteEmbeds()
    {
        foreach (var note in _notes.Values.ToArray()) RetireNoteEmbed(note);
    }

    private void UntrackNoteEmbed(MountedNote note)
    {
        var notes = note.Parent?.Children ?? _notes;
        if (notes.TryGetValue(note.Slot.ObjectId, out var current) && ReferenceEquals(current, note))
            notes.Remove(note.Slot.ObjectId);
    }

    private void RetireNoteEmbed(MountedNote note)
    {
        if (note.Retired) return;
        note.Retired = true;
        UntrackNoteEmbed(note);
        note.Lifetime.Cancel();
        try { DetachNoteEmbed(note); }
        finally { _retirement = Task.WhenAll(_retirement, RetireNoteLoadAsync(note)); }
    }

    private async Task RetireNoteLoadAsync(MountedNote note)
    {
        try { await Task.WhenAll(note.Work, note.DraftWork); }
        finally
        {
            ReleaseNoteAdmission(note);
            note.Lifetime.Dispose();
        }
    }

    private void DetachNoteEmbed(MountedNote note)
    {
        foreach (var nested in note.Children.Values.ToArray()) RetireNoteEmbed(nested);
        if (note.Child is not MarkdownView child) return;
        _noteHosts.Remove(child);
        note.Child = null;
        note.PendingLink = null;
        note.LinkRequest = null;
        child.LinkRequested -= NoteEmbedLinkRequested;
        child.CodeCopied -= PreviewCodeCopied;
        child.ViewportChanged -= NoteEmbedViewportChanged;
        child.VerticalScrollChanged -= NoteEmbedScrollChanged;
        child.VerticalScrollStarting -= NoteEmbedScrollStarting;
        child.Unloaded -= NoteEmbedUnloaded;
        try
        {
            child.Cancel();
            child.SetImageProvider(null, 2);
        }
        finally
        {
            try { note.Owner.SetNoteEmbedChild(note.Document, note.Revision, note.TextGeneration, note.Slot.ObjectId, null); }
            finally
            {
                if (note.Images is MarkdownImageProvider images)
                {
                    note.Images = null;
                    images.Dispose();
                    _retirement = Task.WhenAll(_retirement, images.Completion);
                }
            }
        }
    }

    private void ApplyNoteTheme(MarkdownTheme theme)
    {
        _noteTheme = theme;
        foreach (var note in _noteHosts.Values.ToArray()) note.Child?.SetTheme(theme);
    }

    private static void ClearNoteBranchLinks(MountedNote note)
    {
        // Keep the cached views, while permanently retiring the actions
        // started before this branch left the viewport.
        note.LinkRequest = null;
        note.PendingLink = null;
        foreach (var child in note.Children.Values) ClearNoteBranchLinks(child);
    }

    private bool IsCurrentVisibleNote(MountedNote note)
    {
        for (var ancestor = note; ancestor != null; ancestor = ancestor.Parent)
            if (ancestor.OffscreenSince.HasValue || ancestor.Child is not { Visibility: Visibility.Visible }) return false;
        return IsCurrentNote(note);
    }

    private ulong BeginNoteLinkRequest(MountedNote note)
    {
        var request = ++_linkRequest;
        _pendingFragment = null;
        note.LinkRequest = request;
        return request;
    }

    private bool IsCurrentNoteLink(MountedNote note, ulong request) =>
        request == _linkRequest && note.LinkRequest == request && IsCurrentVisibleNote(note);

    private async void NoteEmbedLinkRequested(MarkdownView sender, MarkdownLinkRequestedEventArgs args)
    {
        if (!_noteHosts.TryGetValue(sender, out var note) || !IsCurrentVisibleNote(note) || !IsCurrentChildLink(sender, args)) return;
        var request = BeginNoteLinkRequest(note);
        try
        {
            if (args.LocalHeadingNode != 0 || args.LocalPreviewPosition.HasValue || args.Destination.StartsWith('#'))
            {
                note.PendingLink = (args, request);
                ApplyNoteLink(note);
            }
            else
            {
                note.PendingLink = null;
                await OpenPreviewDestinationAsync(args.Destination, args.IsWikiLink, note.Origin.DocumentPath,
                    () => IsCurrentNoteLink(note, request) && IsCurrentChildLink(sender, args));
            }
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            if (IsCurrentNoteLink(note, request)) ShowStatus("MarkdownPreview_LinkFailed");
        }
    }

    private void NoteEmbedScrollStarting(MarkdownView sender, object args)
    {
        if (!_noteHosts.TryGetValue(sender, out var note) || !IsCurrentVisibleNote(note)) return;
        note.PendingLink = null;
        CancelPendingFragment();
    }

    private void NoteEmbedViewportChanged(MarkdownView sender, MarkdownViewportAnchor anchor)
    {
        if (anchor.IsAvailable && _noteHosts.TryGetValue(sender, out var note) && IsCurrentNote(note))
        {
            ApplyNoteLink(note);
            RefreshNoteEmbeds(sender, note.Origin, note);
        }
    }

    private void NoteEmbedScrollChanged(MarkdownView sender, object args)
    {
        if (_noteHosts.TryGetValue(sender, out var note)) ApplyNoteLink(note);
    }

    private void NoteEmbedUnloaded(object sender, RoutedEventArgs args)
    {
        if (sender is not MarkdownView child) return;
        _ = Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
        {
            if (child.Parent == null && _noteHosts.TryGetValue(child, out var note))
                RetireNoteEmbed(note);
        });
    }

    private static bool IsCurrentChildLink(MarkdownView child, MarkdownLinkRequestedEventArgs args)
    {
        var state = child.GetSelection();
        return state.IsAvailable && state.DocumentId == args.DocumentId && state.Revision == args.Revision &&
            state.TextGeneration == args.TextGeneration;
    }

    private void ApplyNoteLink(MountedNote note)
    {
        if (note.PendingLink is not { } pending || note.Child is not { } child) return;
        note.PendingLink = null;
        var (args, request) = pending;
        if (!IsCurrentNoteLink(note, request) || !IsCurrentChildLink(child, args)) return;
        try
        {
            var document = args.DocumentId;
            var revision = args.Revision;
            var generation = args.TextGeneration;
            var title = args.IsWikiLink && args.Destination.StartsWith('#') ? args.Destination[1..].Trim() : null;
            var result = args switch
            {
                { LocalHeadingNode: > 0 } => child.NavigateToHeadingNode(document, revision, generation, args.LocalHeadingNode),
                { LocalPreviewPosition: ulong position } => child.NavigateToPreviewPosition(document, revision, generation, position),
                _ when !string.IsNullOrEmpty(title) => child.NavigateToHeadingTitle(document, revision, generation, title),
                _ => child.NavigateToFragment(document, revision, generation, args.Destination)
            };
            if (!IsCurrentNoteLink(note, request)) return;
            if (result == MarkdownFragmentResult.Pending) note.PendingLink ??= pending;
            else if (result == MarkdownFragmentResult.NotFound) ShowStatus("MarkdownPreview_LinkFailed");
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            if (IsCurrentNoteLink(note, request)) ShowStatus("MarkdownPreview_LinkFailed");
        }
    }
}
