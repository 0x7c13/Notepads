// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.DiffViewer;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The document coordinator owns preparing generations; CloseSideBySideDiffViewer disposes ready generations, including shutdown.")]
    private DiffGeneration _diffGeneration;

    private sealed class DiffGeneration : IDisposable
    {
        public CancellationTokenSource Cancellation { get; }
        public SideBySideDiffViewer Viewer { get; } = new();
        public bool Preparing { get; set; } = true;
        public bool PreviousEnabled { get; }
        public long SourceRow { get; }
        public long SourceHorizontalOffset { get; }
        public DiffGeneration(TextEditorCore source, CancellationToken lifetime)
        {
            Cancellation = CancellationTokenSource.CreateLinkedTokenSource(lifetime);
            PreviousEnabled = source.IsEnabled;
            SourceRow = source.FirstVisibleRow;
            SourceHorizontalOffset = source.HorizontalScrollOffset;
        }
        public void Dispose()
        {
            try { Viewer.Dispose(); }
            finally { Cancellation.Dispose(); }
        }
    }

    public async Task OpenSideBySideDiffViewerAsync()
    {
        if (_disposed) return;
        try { await PrepareDiffPreviewAsync(); }
        catch (OperationCanceledException) when (_disposed) { }
        catch (ObjectDisposedException) when (_disposed) { }
        catch (Exception exception)
        {
            LoggingService.LogException(exception);
            if (!_disposed) NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_Unavailable"), 5000);
        }
    }

    private Task PrepareDiffPreviewAsync() => RunDocumentOperationAsync(async cancellation =>
    {
        if (_disposed || _diffGeneration != null || LastSavedSnapshot == null) return;
        if (!TextEditorCore.IsDocumentModified)
        {
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_NoChanges"), 2500);
            return;
        }
        using var baseline = LastSavedSnapshot.Baseline.Retain();
        var generation = new DiffGeneration(TextEditorCore, cancellation);
        _diffGeneration = generation;
        var published = false;
        try
        {
            TextEditorCore.IsEnabled = false;
            generation.Viewer.CloseRequested += DiffViewer_CloseRequested;
            generation.Viewer.ZoomChanged += DiffViewer_ZoomChanged;
            var prepared = await generation.Viewer.PrepareAsync(baseline, TextEditorCore,
                DocumentLanguage, EditingFileName ?? FileNamePlaceholder, generation.Cancellation.Token);
            if (!prepared)
            {
                if (!_disposed && !generation.Cancellation.IsCancellationRequested)
                    NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_Unavailable"), 5000);
                return;
            }
            generation.Cancellation.Token.ThrowIfCancellationRequested();
            if (_disposed || !ReferenceEquals(_diffGeneration, generation)) return;
            if (!generation.Viewer.HasChanges)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_NoChanges"), 2500);
                return;
            }
            DiffPresenter.Content = generation.Viewer;
            EditorRowDefinition.Height = new GridLength(0);
            SideBySideDiffViewRowDefinition.Height = new GridLength(1, GridUnitType.Star);
            generation.Preparing = false;
            Mode = TextEditorMode.DiffPreview;
            published = true;
            generation.Viewer.FocusEditor();
        }
        catch (OperationCanceledException) when (generation.Cancellation.IsCancellationRequested) { }
        catch (Exception exception)
        {
            LoggingService.LogException(exception);
            if (!_disposed) NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("DiffViewer_Unavailable"), 5000);
        }
        finally
        {
            if (!published)
            {
                if (ReferenceEquals(_diffGeneration, generation)) CloseSideBySideDiffViewer();
                generation.Preparing = false;
                generation.Dispose();
                if (!_disposed)
                {
                    TextEditorCore.IsEnabled = generation.PreviousEnabled;
                    RestoreDiffSourceViewport(generation);
                }
            }
        }
    });

    public void CloseSideBySideDiffViewer()
    {
        var generation = _diffGeneration;
        if (generation == null) return;
        _diffGeneration = null;
        generation.Cancellation.Cancel();
        generation.Viewer.CloseRequested -= DiffViewer_CloseRequested;
        generation.Viewer.ZoomChanged -= DiffViewer_ZoomChanged;
        DiffPresenter.Content = null;
        SideBySideDiffViewRowDefinition.Height = new GridLength(0);
        EditorRowDefinition.Height = new GridLength(1, GridUnitType.Star);
        Mode = TextEditorMode.Editing;
        if (generation.Preparing) return; // Its coordinator operation drains and disposes it.
        generation.Dispose();
        if (!_disposed)
        {
            TextEditorCore.IsEnabled = generation.PreviousEnabled;
            RestoreDiffSourceViewport(generation);
        }
    }

    private void RestoreDiffSourceViewport(DiffGeneration generation)
    {
        TextEditorCore.SetDiffViewport(generation.SourceRow, generation.SourceHorizontalOffset);
        TextEditorCore.Focus(FocusState.Programmatic);
    }

    private void DiffViewer_CloseRequested(object sender, EventArgs args) => CloseSideBySideDiffViewer();
    private void DiffViewer_ZoomChanged(object sender, double zoom) => TextEditorCore.SetFontZoomFactor(zoom);
    public async Task ToggleDiffPreviewAsync()
    {
        if (_diffGeneration != null) CloseSideBySideDiffViewer();
        else await OpenSideBySideDiffViewerAsync();
    }
}
