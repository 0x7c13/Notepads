// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Infrastructure.Diagnostics;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    private DispatcherTimer _contentComparisonTimer;
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The comparison owns and disposes its cancellation source; shutdown cancels and awaits it.")]
    private CancellationTokenSource _contentComparisonCancellation;
    private Task _contentComparisonCompletion = Task.CompletedTask;
    private long _comparisonVersion = -1;
    private Guid _comparisonGeneration;
    private long _comparedVersion = -1;
    private Guid _comparedGeneration;
    private bool _comparedEqual;

    private bool HasCurrentContentComparison => _comparedVersion == TextEditorCore.ContentVersion &&
        _comparedGeneration == LastSavedSnapshot.Baseline.GenerationId;

    // Content equality supplements the undo save point; it never moves that point.
    private bool IsTextModified => TextEditorCore.IsDocumentModified &&
        !(TextEditorCore.DocumentLength == 0 && LastSavedSnapshot.Baseline.ByteLength == 0) &&
        !(HasCurrentContentComparison && _comparedEqual);

    private bool NeedsContentComparison => !_disposed && _loaded && TextEditorCore.IsDocumentModified &&
        TextEditorCore.DocumentLength > 0 && TextEditorCore.DocumentLength == LastSavedSnapshot.Baseline.ByteLength &&
        !HasCurrentContentComparison;

    private bool IsComparingCurrentContent => _contentComparisonCancellation is { IsCancellationRequested: false } &&
        _comparisonVersion == TextEditorCore.ContentVersion && _comparisonGeneration == LastSavedSnapshot.Baseline.GenerationId;

    private void RefreshModificationState()
    {
        if (_disposed || !_loaded) return;
        IsModified = !NoChangesSinceLastSaved();
        if (!NeedsContentComparison) { StopContentComparison(); return; }
        if (IsComparingCurrentContent) return;
        StopContentComparison();
        if (_contentComparisonTimer == null)
        {
            _contentComparisonTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(300) };
            _contentComparisonTimer.Tick += ContentComparisonTimerTick;
        }
        _contentComparisonTimer.Start();
    }

    private void ContentComparisonTimerTick(object sender, object args) => StartContentComparison();

    private void StopContentComparison()
    {
        _contentComparisonTimer?.Stop();
        _contentComparisonCancellation?.Cancel();
        if (_disposed && _contentComparisonTimer != null)
        {
            _contentComparisonTimer.Tick -= ContentComparisonTimerTick;
            _contentComparisonTimer = null;
        }
    }

    private Task StartContentComparison()
    {
        _contentComparisonTimer?.Stop();
        if (!NeedsContentComparison) return _contentComparisonCompletion;
        if (IsComparingCurrentContent) return _contentComparisonCompletion;
        StopContentComparison();
        var baseline = LastSavedSnapshot.Baseline.Retain();
        var cancellation = new CancellationTokenSource();
        _contentComparisonCancellation = cancellation;
        _comparisonVersion = TextEditorCore.ContentVersion;
        _comparisonGeneration = baseline.GenerationId;
        // Drain canceled I/O before starting another reader/buffer pair.
        _contentComparisonCompletion = CompareSavedContentAsync(_contentComparisonCompletion,
            baseline, _comparisonVersion, cancellation);
        return _contentComparisonCompletion;
    }

    private async Task RefreshModificationStateAsync()
    {
        RefreshModificationState();
        await StartContentComparison();
    }

    private async Task CompareSavedContentAsync(Task previous, DocumentBaseline baseline, long version,
        CancellationTokenSource cancellation)
    {
        using (baseline)
        using (cancellation)
        {
            var token = cancellation.Token;
            try
            {
                await previous;
                token.ThrowIfCancellationRequested();
                // Always return to the dispatcher before touching live text. A
                // small chunk per turn lets typing/IME run even with cached I/O.
                await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                token.ThrowIfCancellationRequested();
                if (!ContentComparisonIsCurrent(baseline, version)) return;
                using var saved = await baseline.OpenReadStreamAsync(token);
                token.ThrowIfCancellationRequested();
                // The native range reader requires room for one UTF-8 scalar.
                // Small documents should not pay for two full 64 KiB buffers.
                var chunkSize = (int)Math.Clamp(baseline.ByteLength, 4, 65536);
                var expected = new byte[chunkSize];
                var actual = new byte[chunkSize];
                OnContentComparisonBuffersAllocated(chunkSize);
                var equal = true;
                long offset = 0;
                while (offset < baseline.ByteLength)
                {
                    token.ThrowIfCancellationRequested();
                    if (!ContentComparisonIsCurrent(baseline, version)) return;
                    // ReadUtf8Range does not acquire the save/export read lease,
                    // so it cannot freeze editing while the baseline is read.
                    var bytes = TextEditorCore.ReadCurrentUtf8Range(offset, chunkSize);
                    var count = checked((int)bytes.Length);
                    if (count == 0 || count > baseline.ByteLength - offset)
                        throw new System.IO.InvalidDataException("The content comparison returned an invalid range.");
                    bytes.CopyTo(0, actual, 0, count);
                    await saved.ReadExactlyAsync(expected.AsMemory(0, count), token);
                    Task pause = null;
                    OnContentComparisonChunk(offset, count, ref pause);
                    if (pause != null) await pause;
                    token.ThrowIfCancellationRequested();
                    if (!ContentComparisonIsCurrent(baseline, version)) return;
                    equal = actual.AsSpan(0, count).SequenceEqual(expected.AsSpan(0, count));
                    if (!equal) break;
                    offset += count;
                    if (offset < baseline.ByteLength)
                        await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () => { });
                }
                if (!ContentComparisonIsCurrent(baseline, version)) return;
                _comparedVersion = version;
                _comparedGeneration = baseline.GenerationId;
                _comparedEqual = equal;
                RefreshModificationState();
            }
            catch (OperationCanceledException) when (token.IsCancellationRequested) { }
            catch (Exception error)
            {
                // Failure to prove equality must never discard unsaved work.
                LoggingService.LogError($"[{nameof(TextEditor)}] Could not compare saved content: {error.Message}");
            }
            finally
            {
                if (ReferenceEquals(_contentComparisonCancellation, cancellation))
                    _contentComparisonCancellation = null;
            }
        }
    }

    private bool ContentComparisonIsCurrent(DocumentBaseline baseline, long version) => !_disposed && _loaded &&
        TextEditorCore.ContentVersion == version && LastSavedSnapshot.Baseline.GenerationId == baseline.GenerationId;

    // Source-linked tests may pause at real chunk boundaries; erased in production.
    partial void OnContentComparisonChunk(long offset, int count, ref Task pause);
    partial void OnContentComparisonBuffersAllocated(int size);
}
