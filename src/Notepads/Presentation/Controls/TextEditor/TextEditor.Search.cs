// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "This field borrows the active search's using-scoped source; Dispose cancels it and the search disposes it after unwinding.")]
    private CancellationTokenSource _searchCancellation;
    private long _searchRequestVersion;

    private void CancelSearch()
    {
        _searchRequestVersion++;
        _searchCancellation?.Cancel();
    }

    private Task RunDocumentOperationAsync(Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken = default)
    {
        CancelSearch();
        return _documentOperations.RunAsync(operation, cancellationToken);
    }

    private async Task<bool> InitiateFindAndReplaceAsync(FindAndReplaceEventArgs request)
    {
        if (_disposed || !TextEditorCore.IsEnabled || Mode != TextEditorMode.Editing ||
            request.SearchContext == null || string.IsNullOrEmpty(request.SearchContext.SearchText))
        {
            return false;
        }

        CancelSearch();
        var version = _searchRequestVersion;
        using (var cancellation = new CancellationTokenSource())
        {
            _searchCancellation = cancellation;
            if (FindAndReplacePlaceholder?.Visibility == Visibility.Visible)
                _lastSearchContext = request.SearchContext;

            var result = new EditorSearchResult { Status = EditorSearchStatus.Canceled };
            try
            {
                await _documentOperations.RunAsync(async token =>
                {
                    if (_disposed || version != _searchRequestVersion || Mode != TextEditorMode.Editing) return;
                    var previous = request.SearchDirection == SearchDirection.Previous;
                    switch (request.FindAndReplaceMode)
                    {
                        case FindAndReplaceMode.FindOnly:
                            result = await TextEditorCore.FindAsync(request.SearchContext, previous, false, token);
                            break;
                        case FindAndReplaceMode.Replace:
                            result = await TextEditorCore.ReplaceAsync(request.SearchContext, request.ReplaceText, previous, token);
                            break;
                        case FindAndReplaceMode.ReplaceAll:
                            result = await TextEditorCore.ReplaceAllAsync(request.SearchContext, request.ReplaceText, token);
                            break;
                    }
                }, cancellation.Token);
            }
            catch (OperationCanceledException) { return false; }
            catch (ObjectDisposedException) when (_disposed) { return false; }
            catch (Exception error)
            {
                LoggingService.LogError($"[{nameof(TextEditor)}] Search failed: {error.Message}");
                result.Status = EditorSearchStatus.Failed;
            }
            finally
            {
                if (_searchCancellation == cancellation) _searchCancellation = null;
            }

            if (_disposed || cancellation.IsCancellationRequested || version != _searchRequestVersion) return false;
            string resourceKey;
            switch (result.Status)
            {
                case EditorSearchStatus.Found: return true;
                case EditorSearchStatus.NotFound: resourceKey = "FindAndReplace_NotificationMsg_NotFound"; break;
                case EditorSearchStatus.InvalidPattern: resourceKey = "FindAndReplace_NotificationMsg_InvalidRegex"; break;
                case EditorSearchStatus.InvalidReplacement: resourceKey = "FindAndReplace_NotificationMsg_InvalidReplacement"; break;
                case EditorSearchStatus.TimedOut: resourceKey = "FindAndReplace_NotificationMsg_SearchTimedOut"; break;
                case EditorSearchStatus.ResourceLimit: resourceKey = "FindAndReplace_NotificationMsg_SearchLimit"; break;
                case EditorSearchStatus.Failed: resourceKey = "FindAndReplace_NotificationMsg_SearchFailed"; break;
                default: return false;
            }
            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString(resourceKey), 2000);
            return false;
        }
    }
}
