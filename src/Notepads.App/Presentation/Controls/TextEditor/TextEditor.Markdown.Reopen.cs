// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.Markdown;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    // XAML owners never cross apartments. One optional entry per UI dispatcher
    // also bounds retention when many editors close their previews in succession.
    [ThreadStatic]
    private static MarkdownReopenCache _markdownReopenCache;

    private void StopContentPreviewForReopen()
    {
        var extension = _markdownPreview;
        _markdownPreview = null;
        _isContentPreviewPanelOpened = false;
        try
        {
            if (extension != null && extension.TrySuspendForExplicitReopen() &&
                (_markdownReopenCache ??= new MarkdownReopenCache(Dispatcher)).TryStore(this, extension))
                return;
        }
        catch (Exception error) { LoggingService.LogException(error); }
        finally
        {
            if (SplitPanel != null) SplitPanel.Content = null;
        }
        if (extension != null) RetireMarkdownPreview(extension, discardDocument: true);
    }

    private void RetireMarkdownPreview(MarkdownExtensionView extension, bool discardDocument = false)
    {
        try
        {
            if (discardDocument) extension.DiscardRetainedDocument();
        }
        catch (Exception error) { LoggingService.LogException(error); }
        finally
        {
            extension.Dispose();
            _previewShutdown = Task.WhenAll(_previewShutdown, extension.DisposalCompletion);
        }
    }

    private sealed class MarkdownReopenCache
    {
        internal const ulong MaximumRetainedBytes = 32UL * 1024 * 1024;
        private static readonly TimeSpan Lifetime = TimeSpan.FromSeconds(30);
        private readonly CoreDispatcher _dispatcher;
        [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
            Justification = "Eviction retires the borrowed editor's preview; editor disposal evicts its own entry.")]
        private TextEditor _owner;
        [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
            Justification = "Take transfers this preview back; every eviction calls its owning editor's retirement path.")]
        private MarkdownExtensionView _preview;
        private DispatcherTimer _expiry;
        private long _storedAt, _serial;
        private bool _pressureSubscribed;

        internal MarkdownReopenCache(CoreDispatcher dispatcher) => _dispatcher = dispatcher;

        internal bool Owns(TextEditor editor) => ReferenceEquals(_owner, editor);

        internal bool TryStore(TextEditor editor, MarkdownExtensionView preview)
        {
            if (editor._disposed || MemoryManager.AppMemoryUsageLevel != AppMemoryUsageLevel.Low ||
                !preview.PreviewView.HasCachedDocument ||
                preview.PreviewView.GetRetainedByteEstimate() > MaximumRetainedBytes) return false;
            Evict();
            try
            {
                _owner = editor;
                _preview = preview;
                _storedAt = Stopwatch.GetTimestamp();
                Interlocked.Increment(ref _serial);
                _expiry = new DispatcherTimer { Interval = Lifetime };
                _expiry.Tick += Expired;
                MemoryManager.AppMemoryUsageIncreased += MemoryPressure;
                _pressureSubscribed = true;
                _expiry.Start();
                return true;
            }
            catch
            {
                // The caller still owns and retires this preview on rejection.
                Remove();
                throw;
            }
        }

        private void Expired(object sender, object args)
        {
            if (!ReferenceEquals(sender, _expiry)) return;
            try { Evict(); }
            catch (Exception error) { LoggingService.LogException(error); }
        }

        internal MarkdownExtensionView Take(TextEditor editor)
        {
            if (!Owns(editor)) return null;
            if (Stopwatch.GetElapsedTime(_storedAt) >= Lifetime ||
                MemoryManager.AppMemoryUsageLevel != AppMemoryUsageLevel.Low ||
                !_preview.CanResumeExplicitReopen())
            {
                Evict();
                return null;
            }
            var preview = _preview;
            Remove(); // An already queued expiry/pressure callback cannot retire the active view.
            return preview;
        }

        internal void Evict(TextEditor editor)
        {
            if (Owns(editor)) Evict();
        }

        private void Evict()
        {
            var owner = _owner;
            var preview = _preview;
            Remove();
            if (preview != null) owner.RetireMarkdownPreview(preview, discardDocument: true);
        }

        private void Remove()
        {
            Interlocked.Increment(ref _serial);
            _owner = null;
            _preview = null;
            var expiry = _expiry;
            _expiry = null;
            try
            {
                if (expiry != null)
                {
                    expiry.Tick -= Expired;
                    expiry.Stop();
                }
            }
            finally
            {
                if (_pressureSubscribed)
                {
                    _pressureSubscribed = false;
                    MemoryManager.AppMemoryUsageIncreased -= MemoryPressure;
                }
            }
        }

        private async void MemoryPressure(object sender, object args)
        {
            try
            {
                if (MemoryManager.AppMemoryUsageLevel == AppMemoryUsageLevel.Low) return;
                var serial = Interlocked.Read(ref _serial);
                await _dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
                {
                    if (serial == _serial) Evict();
                });
            }
            catch (Exception error) { LoggingService.LogException(error); }
        }
    }
}
