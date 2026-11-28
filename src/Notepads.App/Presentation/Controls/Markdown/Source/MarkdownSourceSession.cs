// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.Markdown;

internal enum MarkdownSessionStatus { Disabled, Updating, Restoring, Ready, TooLarge, Failed }

internal readonly record struct MarkdownEditorStamp(
    ulong NativeRevision, long Length, long ContentVersion, long Generation);

/// <summary>Owns one preview submission at a time, independently of the editor's undo journal.</summary>
internal sealed partial class MarkdownSourceSession : IDisposable
{
    internal const long MaximumSourceBytes = 4L * 1024 * 1024;
    private const int ReadChunkBytes = 65536;
    private static readonly TimeSpan Debounce = TimeSpan.FromMilliseconds(120);
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The TextEditor owns its core and waits for this session's Completion before disposing it.")]
    private readonly TextEditorCore _editor;
    private readonly MarkdownView _preview;
    private readonly MarkdownDirtySpan _pending = new();
    private TaskCompletionSource<bool> _wake = CreateWake();
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "Stop cancels the worker; its finally disposes the source after every operation has unwound.")]
    private CancellationTokenSource _cancellation;
    private Task _completion = Task.CompletedTask;
    private bool _enabled;
    private bool _disposed;
    private bool _needsFull = true;
    private bool _waitForChange;
    private FailedPublication _failedPublication;
    private long _generation;
    private long _changeVersion;
    // Only edits and retries start the debounce, including across a quick close and reopen.
    private long? _lastChangeTicks;
    private long _logicalLength;
    private ulong _logicalNativeRevision;
    private long _logicalContentVersion;
    private ulong _nextRevision;
    private MarkdownExtensions _extensions = ExtensionsFor(MarkdownFrontMatterStyle.Table);

    internal MarkdownSourceSession(TextEditorCore editor, MarkdownView preview)
    {
        _editor = editor ?? throw new ArgumentNullException(nameof(editor));
        _preview = preview ?? throw new ArgumentNullException(nameof(preview));
    }

    internal Guid DocumentId { get; } = Guid.NewGuid();
    internal ulong AcceptedRevision { get; private set; }
    internal MarkdownEditorStamp AcceptedStamp { get; private set; }
    internal MarkdownExtensions AcceptedExtensions { get; private set; }
    internal MarkdownExtensions Extensions => _extensions;
    internal MarkdownSessionStatus Status { get; private set; } = MarkdownSessionStatus.Disabled;
    internal Task Completion => _completion;
    internal long FullSubmissions { get; private set; }
    internal long DeltaSubmissions { get; private set; }
    internal long CopiedBytes { get; private set; }
    internal event EventHandler Changed;
    internal event Action<AcceptedEdit> EditAccepted;

    // Observers borrow the same immutable edit payload for aligned draft views.
    internal readonly record struct AcceptedEdit(ulong BaseRevision, ulong Revision, long Length,
        MarkdownSourceEdit Edit, byte[] Bytes);

    partial void ObserveSubmissionReceipt(DocumentUpdateResult receipt);

    private sealed record FailedPublication(ulong Revision, MarkdownEditorStamp Stamp, MarkdownExtensions Extensions);

    internal bool IsCurrent
    {
        get
        {
            if (!_enabled || _disposed || _needsFull || AcceptedRevision == 0 ||
                !_editor.MarkdownCanRead || !_preview.IsLoaded || AcceptedStamp.Generation != _generation) return false;
            try
            {
                return MatchesAcceptedSource(allowCache: true) && CaptureStamp(_generation) == AcceptedStamp;
            }
            catch (Exception error)
            {
                LoggingService.LogException(error);
                return false;
            }
        }
    }

    internal bool CanResumeCachedDocument => !_disposed && AcceptedRevision != 0 &&
        _editor.MarkdownCanRead && _preview.HasCachedDocument &&
        CaptureStamp(_generation) == (AcceptedStamp with { Generation = _generation }) &&
        MatchesAcceptedSource(allowCache: true);

    internal void Start()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(MarkdownSourceSession));
        if (_enabled) return;
        var generation = checked(++_generation);
        var stamp = AcceptedStamp with { Generation = generation };
        bool resumeCached = AcceptedRevision != 0 && _editor.MarkdownCanRead && _preview.HasCachedDocument &&
            CaptureStamp(generation) == stamp && MatchesAcceptedSource(allowCache: true);
        if (resumeCached)
        {
            AcceptedStamp = stamp;
            _logicalLength = stamp.Length;
            _logicalNativeRevision = stamp.NativeRevision;
            _logicalContentVersion = stamp.ContentVersion;
        }
        _enabled = true;
        _needsFull = !resumeCached;
        _waitForChange = false;
        _pending.Clear();
        _wake = CreateWake();
        _editor.MarkdownModified += OnModified;
        _editor.MarkdownReset += OnReset;
        _editor.MarkdownAvailabilityChanged += OnReset;
        _preview.Loaded += OnPreviewLoaded;
        _preview.SizeChanged += OnPreviewSizeChanged;
        _preview.ViewportChanged += OnPreviewViewportChanged;
        _preview.InitialViewportChanged += OnPreviewSurfaceChanged;
        var previous = _completion;
        var cancellation = new CancellationTokenSource();
        _cancellation = cancellation;
        // A reopen waits for the old operation to unwind before submitting again.
        _completion = RunAsync(previous, generation, cancellation, resumeCached);
        SetStatus(resumeCached ? MarkdownSessionStatus.Restoring : MarkdownSessionStatus.Updating);
    }

    internal void Stop(bool preserveDocument = false)
    {
        if (!_enabled)
        {
            if (!preserveDocument && _preview.HasCachedDocument) CancelPreview();
            return;
        }
        _enabled = false;
        checked { ++_generation; }
        _editor.MarkdownModified -= OnModified;
        _editor.MarkdownReset -= OnReset;
        _editor.MarkdownAvailabilityChanged -= OnReset;
        _preview.Loaded -= OnPreviewLoaded;
        _preview.SizeChanged -= OnPreviewSizeChanged;
        _preview.ViewportChanged -= OnPreviewViewportChanged;
        _preview.InitialViewportChanged -= OnPreviewSurfaceChanged;
        _pending.Clear();
        _failedPublication = null;
        _needsFull = true;
        _waitForChange = false;
        try { _cancellation?.Cancel(); }
        catch (Exception error) { LoggingService.LogException(error); }
        _wake.TrySetResult(true);
        if (!preserveDocument) CancelPreview();
        SetStatus(MarkdownSessionStatus.Disabled);
    }

    private void CancelPreview()
    {
        try { _preview.Cancel(); }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    internal void SetFrontMatterStyle(MarkdownFrontMatterStyle style)
    {
        if (_disposed) return;
        var next = ExtensionsFor(style);
        if (_extensions == next) return;
        _extensions = next;
        RequestFullSync();
    }

    private static MarkdownExtensions ExtensionsFor(MarkdownFrontMatterStyle style) =>
        MarkdownExtensions.Math | MarkdownExtensions.Mermaid | MarkdownExtensions.HtmlBlocks | MarkdownExtensions.HtmlInline |
        MarkdownExtensions.HeadingIds |
        MarkdownExtensions.Footnotes |
        MarkdownExtensions.Callouts |
        MarkdownExtensions.DescriptionLists |
        MarkdownExtensions.Highlight |
        MarkdownExtensions.InlineScripts |
        MarkdownExtensions.EmojiShortcodes |
        MarkdownExtensions.TableOfContents |
        MarkdownExtensions.LocalWikiLinks |
        MarkdownExtensions.NoteEmbeds |
        MarkdownExtensions.Citations |
        (style switch
        {
            MarkdownFrontMatterStyle.Table => MarkdownExtensions.FrontMatter,
            MarkdownFrontMatterStyle.CodeBlock => MarkdownExtensions.FrontMatterCode,
            MarkdownFrontMatterStyle.Hide => MarkdownExtensions.FrontMatterHide,
            _ => throw new ArgumentOutOfRangeException(nameof(style))
        });

    internal void RequestFullSync()
    {
        if (_disposed) return;
        RequireFullSync();
        if (!_enabled) return;
        MarkChanged();
    }

    public void Dispose()
    {
        if (_disposed) return;
        Stop();
        _disposed = true;
    }

    private static TaskCompletionSource<bool> CreateWake() =>
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    private bool IsActive(long generation) => _enabled && !_disposed && _generation == generation;

    private MarkdownEditorStamp CaptureStamp(long generation) => new(
        _editor.MarkdownNativeRevision, _editor.MarkdownUtf8Length, _editor.ContentVersion, generation);

    private bool Matches(MarkdownEditorStamp stamp) => IsActive(stamp.Generation) &&
        _editor.MarkdownCanRead && CaptureStamp(stamp.Generation) == stamp;

    private void OnModified(object sender, MarkdownTextModification modification)
    {
        if (!_enabled || _disposed) return;
        try
        {
            if (!_needsFull)
            {
                if (modification.Position < 0 || modification.DeleteLength < 0 || modification.InsertLength < 0 ||
                    modification.Position > _logicalLength || modification.DeleteLength > _logicalLength - modification.Position)
                    throw new InvalidDataException("The editor modification does not belong to the pending source.");
                var length = checked(_logicalLength - modification.DeleteLength + modification.InsertLength);
                _pending.Add(modification.Position, modification.DeleteLength, modification.InsertLength);
                _logicalLength = length;
                _logicalNativeRevision = checked(_logicalNativeRevision + 1);
                _logicalContentVersion = checked(_logicalContentVersion + 1);
            }
            MarkChanged();
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            RequireFullSync();
            MarkChanged();
        }
    }

    private void OnReset(object sender, EventArgs args) => RequestFullSync();

    private void OnPreviewLoaded(object sender, RoutedEventArgs args) => Wake();

    private void OnPreviewSizeChanged(object sender, SizeChangedEventArgs args) => Wake();

    private void OnPreviewViewportChanged(MarkdownView sender, MarkdownViewportAnchor anchor)
        => OnPreviewSurfaceChanged(sender, null);

    private void OnPreviewSurfaceChanged(MarkdownView sender, object args)
    {
        try { RecoverFailedPublication(); }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    private void RecoverFailedPublication()
    {
        // A later graphics retry can paint an already accepted full submission.
        // It can recover only that exact host admission, never adopt native source.
        if (_failedPublication is not { } failed || !_enabled || _disposed ||
            Status != MarkdownSessionStatus.Failed || !_waitForChange || !_needsFull || _pending.HasChanges ||
            failed.Revision != AcceptedRevision || failed.Stamp != AcceptedStamp || failed.Extensions != _extensions ||
            !Matches(failed.Stamp) || !MatchesAcceptedSource() || !_preview.IsLoaded || !_preview.IsSurfaceReady ||
            !string.IsNullOrEmpty(_preview.RenderingError) || _preview.DisplayedDocumentId != DocumentId ||
            _preview.DisplayedRevision != failed.Revision) return;
        var selection = _preview.GetSelection();
        if (!selection.IsAvailable || selection.DocumentId != DocumentId || selection.Revision != failed.Revision) return;
        _failedPublication = null;
        _needsFull = _waitForChange = false;
        SetStatus(MarkdownSessionStatus.Ready);
    }

    internal void Wake()
    {
        if (_enabled && !_disposed) _wake.TrySetResult(true);
    }

    private async Task WaitForWakeAsync(CancellationToken token)
    {
        var wake = _wake;
        await wake.Task.WaitAsync(token);
        if (ReferenceEquals(_wake, wake)) _wake = CreateWake();
    }

    private void RequireFullSync()
    {
        _failedPublication = null;
        _needsFull = true;
        _pending.Clear();
        _waitForChange = false;
    }

    private void MarkChanged()
    {
        _failedPublication = null;
        _changeVersion = checked(_changeVersion + 1);
        _lastChangeTicks = Stopwatch.GetTimestamp();
        _waitForChange = false;
        _wake.TrySetResult(true);
        if (_enabled && _completion.IsCompleted)
        {
            var previous = _completion;
            var cancellation = new CancellationTokenSource();
            _cancellation = cancellation;
            _completion = RunAsync(previous, _generation, cancellation);
        }
        SetStatus(MarkdownSessionStatus.Updating);
        RaiseChanged();
    }

    private async Task RunAsync(Task previous, long generation, CancellationTokenSource cancellation,
        bool resumeCached = false)
    {
        var token = cancellation.Token;
        try
        {
            await previous;
            if (!IsActive(generation)) return;
            // Never sample inside a native modification/TSF notification callback.
            await _editor.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            while (IsActive(generation))
            {
                try
                {
                    token.ThrowIfCancellationRequested();
                    if (!resumeCached && (_waitForChange || (!_needsFull && !_pending.HasChanges)))
                    {
                        await WaitForWakeAsync(token);
                        if (!IsActive(generation)) return;
                        continue;
                    }

                    if (!resumeCached && _lastChangeTicks is long lastChange)
                    {
                        var remaining = Debounce - Stopwatch.GetElapsedTime(lastChange);
                        if (remaining > TimeSpan.Zero)
                        {
                            await Task.Delay(remaining, token);
                            if (!IsActive(generation)) return;
                            continue;
                        }
                    }
                    // Ordinary retained tabs can still accept source/style changes.
                    // A pressure-suspended hidden tab must wait for explicit host resume.
                    if (!_editor.MarkdownCanRead || !_preview.IsLoaded ||
                        (_preview.IsCachedPresentationSuspended && _preview.Visibility != Visibility.Visible) ||
                        _preview.ActualWidth <= 0 || _preview.ActualHeight <= 0)
                    {
                        await WaitForWakeAsync(token);
                        if (!IsActive(generation)) return;
                        continue;
                    }

                    if (resumeCached)
                    {
                        resumeCached = false;
                        if (!_needsFull && !_pending.HasChanges && Matches(AcceptedStamp) && MatchesAcceptedSource(allowCache: true))
                        {
                            var receipt = await _preview.ResumeCachedDocumentAsync(DocumentId, AcceptedRevision)
                                .AsCompletionTask(token);
                            if (!IsActive(generation)) return;
                            if (receipt.Status == DocumentUpdateStatus.Ready && receipt.Published)
                            {
                                if (!_needsFull && !_pending.HasChanges) SetStatus(MarkdownSessionStatus.Ready);
                                continue;
                            }
                            if (receipt.Status == DocumentUpdateStatus.NotLoaded)
                            {
                                resumeCached = true;
                                await Task.Delay(30, token);
                                continue;
                            }
                        }
                        RequireFullSync();
                        SetStatus(MarkdownSessionStatus.Updating);
                    }

                    var submission = await CaptureSubmissionAsync(generation, token);
                    if (!IsActive(generation)) return;
                    if (submission == null) continue;
                    await SubmitAsync(submission, generation, token);
                    if (!IsActive(generation)) return;
                }
                catch (OperationCanceledException) when (token.IsCancellationRequested) { throw; }
                catch (Exception error)
                {
                    LoggingService.LogException(error);
                    if (!IsActive(generation)) return;
                    RequireFullSync();
                    _waitForChange = true;
                    SetStatus(MarkdownSessionStatus.Failed);
                }
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested) { }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            if (IsActive(generation))
            {
                RequireFullSync();
                SetStatus(MarkdownSessionStatus.Failed);
            }
        }
        finally
        {
            if (ReferenceEquals(_cancellation, cancellation)) _cancellation = null;
            cancellation.Dispose();
        }
    }

    private async Task<Submission> CaptureSubmissionAsync(long generation, CancellationToken token)
    {
        var stamp = CaptureStamp(generation);
        if (stamp.Length < 0 || stamp.Length > MaximumSourceBytes)
        {
            RequireFullSync();
            _waitForChange = true;
            SetStatus(MarkdownSessionStatus.TooLarge);
            return null;
        }
        if (!_needsFull && (_logicalLength != stamp.Length || _logicalNativeRevision != stamp.NativeRevision ||
            _logicalContentVersion != stamp.ContentVersion)) RequireFullSync();

        var full = _needsFull || !_pending.HasChanges ||
            (_pending.DeleteLength == 0 && _pending.ReplacementLength == 0) || !HasAcceptedSource();
        var start = full ? 0 : _pending.Start;
        var deleteLength = full ? 0 : _pending.DeleteLength;
        var count = full ? stamp.Length : _pending.ReplacementLength;
        if (start < 0 || count < 0 || start > stamp.Length || count > stamp.Length - start ||
            (!full && (start > AcceptedStamp.Length || deleteLength > AcceptedStamp.Length - start)))
        {
            RequireFullSync();
            return null;
        }
        var version = _changeVersion;
        var extensions = _extensions;
        var bytes = await ReadEditorRangeAsync(stamp, version, start, count, token);
        return bytes == null ? null : new Submission(stamp, full, start, deleteLength, bytes, version, extensions);
    }

    internal Task<byte[]> ReadAcceptedNoteSourceAsync(ulong revision, CancellationToken token)
    {
        if (revision != AcceptedRevision || !IsCurrent)
            return Task.FromResult<byte[]>(null);
        var stamp = AcceptedStamp;
        return ReadEditorRangeAsync(stamp, _changeVersion, 0, stamp.Length, token, forSubmission: false);
    }

    private async Task<byte[]> ReadEditorRangeAsync(MarkdownEditorStamp stamp, long version, long start, long count,
        CancellationToken token, bool forSubmission = true)
    {
        var bytes = new byte[checked((int)count)];
        var copied = 0;
        while (copied < bytes.Length)
        {
            token.ThrowIfCancellationRequested();
            if (!Matches(stamp) || version != _changeVersion) return null;
            var remaining = bytes.Length - copied;
            var owned = _editor.ReadCurrentUtf8Range(start + copied, Math.Max(4, Math.Min(ReadChunkBytes, remaining)));
            var received = checked((int)owned.Length);
            var take = Math.Min(remaining, received);
            if (take == 0) throw new InvalidDataException("The native Markdown range reader returned an empty non-EOF range.");
            owned.CopyTo(0, bytes, copied, take);
            if (forSubmission) CopiedBytes = checked(CopiedBytes + take);
            copied += take;
            // Draft note callers may mount a child. Keep their continuation
            // outside the parent's synchronous viewport notification.
            if (forSubmission && copied == bytes.Length) break;
            if (!Matches(stamp) || version != _changeVersion) return null;
            // The owned block has no native read lease; typing can run between blocks.
            await _editor.Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
            if (!IsActive(stamp.Generation)) return null;
        }
        if (!Matches(stamp) || version != _changeVersion) return null;
        return bytes;
    }

    private bool HasAcceptedSource()
    {
        return AcceptedRevision != 0 && AcceptedStamp.Generation == _generation && MatchesAcceptedSource();
    }

    private bool MatchesAcceptedSource(bool allowCache = false)
    {
        var source = _preview.GetSourceState();
        return AcceptedExtensions == _extensions && source.StateKnown && source.HasSource &&
            (!source.NeedsResync || (allowCache && _preview.HasCachedDocument)) &&
            source.DocumentId == DocumentId && source.Revision == AcceptedRevision;
    }

    private async Task SubmitAsync(Submission submission, long generation, CancellationToken token)
    {
        if (!IsActive(generation) || !Matches(submission.Stamp) ||
            submission.ChangeVersion != _changeVersion || submission.Extensions != _extensions) return;
        var revision = checked(++_nextRevision);
        var baseRevision = AcceptedRevision;
        // New synchronous or asynchronous modifications now belong to the target source.
        // Do not clear this tail again when the parsing receipt eventually completes.
        _pending.Clear();
        _needsFull = false;
        _logicalLength = submission.Stamp.Length;
        _logicalNativeRevision = submission.Stamp.NativeRevision;
        _logicalContentVersion = submission.Stamp.ContentVersion;
        SetStatus(MarkdownSessionStatus.Updating);
        if (!IsActive(generation)) return;

        DocumentUpdateResult receipt = null;
        Exception failure = null;
        var accepted = false;
        try
        {
            var operation = submission.Full
                ? _preview.SetDocumentWithExtensionsAsync(DocumentId, revision, submission.Bytes,
                    submission.Extensions)
                : _preview.ApplyEditsAsync(DocumentId, baseRevision, revision,
                    [submission.Edit], submission.Bytes);
            if (submission.Full) FullSubmissions = checked(FullSubmissions + 1);
            else DeltaSubmissions = checked(DeltaSubmissions + 1);
            if (!IsActive(generation))
            {
                // Cancellation still waits for the WinRT operation to finish unwinding.
                await operation.AsCompletionTask(token);
                return;
            }
            accepted = RecordAcceptance(submission, baseRevision, revision);
            receipt = await operation.AsCompletionTask(token);
            ObserveSubmissionReceipt(receipt);
        }
        catch (Exception error)
        {
            failure = error;
            if (error is not OperationCanceledException || !token.IsCancellationRequested)
                LoggingService.LogException(error);
        }
        if (!IsActive(generation)) return;
        // A full submission may have accepted only after its deferred viewport/drain work.
        accepted |= RecordAcceptance(submission, baseRevision, revision);
        if (!IsActive(generation)) return;
        var source = _preview.GetSourceState();
        var exact = source.StateKnown && source.HasSource && source.DocumentId == DocumentId && source.Revision == revision;
        var mustResync = !accepted || !exact || source.NeedsResync || receipt?.Status == DocumentUpdateStatus.ResyncRequired;
        var ready = failure == null && receipt?.Status == DocumentUpdateStatus.Ready && receipt.Published;
        if (mustResync || !ready)
        {
            RequireFullSync();
            var unchanged = _changeVersion == submission.ChangeVersion;
            // One failed delta gets an explicit full recovery. A stable failed full
            // waits for a real edit/request rather than retrying a bad document forever.
            _waitForChange = submission.Full && unchanged && receipt?.Status != DocumentUpdateStatus.NotLoaded;
            if (_waitForChange && accepted && exact && !source.NeedsResync && failure == null &&
                receipt?.Status == DocumentUpdateStatus.Failed && Matches(submission.Stamp) &&
                submission.Extensions == _extensions)
                _failedPublication = new(revision, submission.Stamp, submission.Extensions);
            SetStatus(receipt?.Status == DocumentUpdateStatus.NotLoaded
                ? MarkdownSessionStatus.Updating : MarkdownSessionStatus.Failed);
            // Native recovery may have published before the failed receipt resumed.
            RecoverFailedPublication();
            if (!_waitForChange) _lastChangeTicks = Stopwatch.GetTimestamp();
            return;
        }
        SetStatus(_needsFull || _pending.HasChanges ? MarkdownSessionStatus.Updating : MarkdownSessionStatus.Ready);
        RaiseChanged();
    }

    private bool RecordAcceptance(Submission submission, ulong baseRevision, ulong revision)
    {
        var source = _preview.GetSourceState();
        if (!source.StateKnown || !source.HasSource || source.DocumentId != DocumentId || source.Revision != revision)
            return false;
        var acceptedDelta = !submission.Full && AcceptedRevision == baseRevision;
        AcceptedRevision = revision;
        AcceptedStamp = submission.Stamp;
        // A style change can run while the operation awaits publication. This
        // identity belongs to the actual submitted source, never the live setting.
        AcceptedExtensions = submission.Extensions;
        if (source.NeedsResync || submission.Extensions != _extensions) RequireFullSync();
        else if (acceptedDelta)
        {
            try { EditAccepted?.Invoke(new(baseRevision, revision, submission.Stamp.Length, submission.Edit, submission.Bytes)); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
        RaiseChanged();
        return true;
    }

    private void SetStatus(MarkdownSessionStatus status)
    {
        if (Status == status) return;
        Status = status;
        RaiseChanged();
    }

    private void RaiseChanged()
    {
        if (Changed == null) return;
        foreach (EventHandler subscriber in Changed.GetInvocationList())
        {
            try { subscriber(this, EventArgs.Empty); }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }

    private sealed record Submission(MarkdownEditorStamp Stamp, bool Full, long Start,
        long DeleteLength, byte[] Bytes, long ChangeVersion, MarkdownExtensions Extensions)
    {
        public MarkdownSourceEdit Edit => new()
        {
            Start = checked((ulong)Start),
            DeleteLength = checked((ulong)DeleteLength),
            ReplacementLength = checked((ulong)Bytes.Length)
        };
    }
}
