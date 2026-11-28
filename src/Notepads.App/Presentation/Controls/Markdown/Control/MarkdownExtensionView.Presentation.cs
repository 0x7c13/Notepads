// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using EditorHost = Notepads.Presentation.Controls.TextEditor.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Views.MainPage;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.System;
using Windows.UI;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    private bool _restoringSessionViewport;
    private bool _viewportResourceReleasePending;
    private DispatcherTimer _viewportRestoreDeadline;

    internal void BeginSessionViewportRestore()
    {
        // Keep the native view in layout so it can draw the destination behind the placeholder.
        StopViewportResourceRelease();
        _restoringSessionViewport = true;
        _view.IsResourceRenderingDeferred = true;
        _view.Opacity = 0;
        _view.IsEnabled = false;
    }

    private void FinishSessionViewportRestore()
    {
        if (!_restoringSessionViewport) return;
        _restoringSessionViewport = false;
        StopViewportRestoreDeadline();
        _view.Opacity = 1;
        _view.IsEnabled = true;
        ShowLoading(!_view.IsSurfaceReady &&
            (_session.Status is MarkdownSessionStatus.Updating or MarkdownSessionStatus.Restoring));
        if (_active && !_disposed && _view.IsLoaded && _view.IsSurfaceReady)
        {
            // Give the correctly positioned revealed frame a render opportunity
            // before optional work can invalidate its reading geometry again.
            _viewportResourceReleasePending = true;
            CompositionTarget.Rendering += ViewportResourcesRendering;
        }
        else _view.IsResourceRenderingDeferred = false;
    }

    private void ViewportResourcesRendering(object sender, object args)
    {
        StopViewportResourceRelease();
        if (!_restoringSessionViewport) _view.IsResourceRenderingDeferred = false;
    }

    private void StopViewportResourceRelease()
    {
        if (!_viewportResourceReleasePending) return;
        _viewportResourceReleasePending = false;
        CompositionTarget.Rendering -= ViewportResourcesRendering;
    }

    private void StopViewportRestoreDeadline()
    {
        if (_viewportRestoreDeadline == null) return;
        _viewportRestoreDeadline.Stop();
        _viewportRestoreDeadline.Tick -= ViewportRestoreDeadlineReached;
        _viewportRestoreDeadline = null;
    }

    private void ViewportRestoreDeadlineReached(object sender, object args)
    {
        if (!ReferenceEquals(sender, _viewportRestoreDeadline)) return;
        if (_view.Visibility != Visibility.Visible)
        {
            StopViewportRestoreDeadline();
            return;
        }
        // An unavailable viewport or failed follow draw cannot keep the preview covered.
        CancelSourceLink();
        FinishSessionViewportRestore();
    }

    private void SessionChanged(object sender, EventArgs args)
    {
        if (_disposed) return;
        if (_restoringSessionViewport && _active)
        {
            if (_session.Status is MarkdownSessionStatus.Failed or MarkdownSessionStatus.TooLarge ||
                (_session.Status == MarkdownSessionStatus.Ready && _view.DisplayedTextLength == 0))
                FinishSessionViewportRestore();
            else UpdateViewportRestoreDeadline();
        }
        ShowLoading((_active && _restoringSessionViewport) ||
            ((_session.Status is MarkdownSessionStatus.Updating or MarkdownSessionStatus.Restoring) && !_view.IsSurfaceReady));
        var key = _session.Status switch
        {
            MarkdownSessionStatus.TooLarge => "MarkdownPreview_TooLarge",
            MarkdownSessionStatus.Failed => "MarkdownPreview_Failed",
            _ => null
        };
        ShowStatus(key);
        SourceLinkStateChanged();
        ApplyPendingFragment();
        RefreshNoteEmbeds();
    }

    private void UpdateViewportRestoreDeadline()
    {
        if (!_restoringSessionViewport || !_active || _disposed || _view.Visibility != Visibility.Visible ||
            _session.Status != MarkdownSessionStatus.Ready || _viewportRestoreDeadline != null) return;
        _viewportRestoreDeadline = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
        _viewportRestoreDeadline.Tick += ViewportRestoreDeadlineReached;
        _viewportRestoreDeadline.Start();
    }

    private void InitialViewportChanged(MarkdownView sender, object args)
    {
        if (_active && !_disposed && !_restoringSessionViewport)
            ShowLoading(!sender.IsSurfaceReady &&
                (_session.Status is MarkdownSessionStatus.Updating or MarkdownSessionStatus.Restoring));
        ApplyPendingFragment();
    }

    private readonly record struct FragmentRequest(string Destination, ulong SourceRevision,
        (ulong Position, ulong TextGeneration)? Local = null,
        (uint Node, ulong TextGeneration)? Heading = null,
        bool WikiHeading = false)
    {
        public ulong? TextGeneration => Heading?.TextGeneration ?? Local?.TextGeneration;
    }

    internal void QueueFragment(string fragment, bool wikiHeading = false) =>
        QueueDocumentTarget(new(fragment, _editor.MarkdownNativeRevision, WikiHeading: wikiHeading));

    private void QueueDocumentTarget(FragmentRequest target)
    {
        ++_linkRequest;
        _pendingFragment = target;
        ApplyPendingFragment();
    }

    private void CancelPendingFragment()
    {
        ++_linkRequest;
        _pendingFragment = null;
    }

    private void PendingFragmentScrollChanged(MarkdownView sender, object args) => ApplyPendingFragment();

    private void PendingFragmentViewportChanged(MarkdownView sender, MarkdownViewportAnchor anchor)
    {
        if (anchor.IsAvailable)
        {
            ApplyPendingFragment();
            RefreshNoteEmbeds();
        }
    }

    private void ApplyPendingFragment()
    {
        if (_pendingFragment is not { } fragment || _disposed || !_active || !_linkRunning ||
            _view.Visibility != Visibility.Visible) return;
        if (fragment.SourceRevision != _editor.MarkdownNativeRevision ||
            _session.Status is MarkdownSessionStatus.Failed or MarkdownSessionStatus.TooLarge)
        {
            _pendingFragment = null;
            return;
        }
        if (_session.Status != MarkdownSessionStatus.Ready || !_session.IsCurrent) return;
        var frame = _view.GetSelection();
        if (!frame.IsAvailable || frame.DocumentId != _session.DocumentId || frame.Revision != _session.AcceptedRevision) return;
        if (fragment.TextGeneration is ulong generation && generation != frame.TextGeneration)
        {
            _pendingFragment = null;
            return;
        }

        var request = _linkRequest;
        _pendingFragment = null;
        try
        {
            FinishSessionViewportRestore();
            SetLinkPane(true, LinkIntent.Fragment);
            if (_disposed || !_active || request != _linkRequest) return;
            var result = fragment switch
            {
                { Heading: { } target } => _view.NavigateToHeadingNode(
                    frame.DocumentId, frame.Revision, frame.TextGeneration, target.Node),
                { Local: { } target } => _view.NavigateToPreviewPosition(
                    frame.DocumentId, frame.Revision, frame.TextGeneration, target.Position),
                { WikiHeading: true } => _view.NavigateToHeadingTitle(
                    frame.DocumentId, frame.Revision, frame.TextGeneration, fragment.Destination),
                _ => _view.NavigateToFragment(frame.DocumentId, frame.Revision, frame.TextGeneration, fragment.Destination)
            };
            switch (result)
            {
                case MarkdownFragmentResult.Pending:
                    if (_active && !_disposed && request == _linkRequest) _pendingFragment ??= fragment;
                    break;
                case MarkdownFragmentResult.NotFound:
                    if (request == _linkRequest) ShowStatus("MarkdownPreview_LinkFailed");
                    break;
            }
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            if (!_disposed && request == _linkRequest) ShowStatus("MarkdownPreview_LinkFailed");
        }
    }

    private void ShowLoading(bool loading)
    {
        if (loading && PreviewLoading.Child == null)
        {
            var placeholder = new DocumentLoadingPlaceholder();
            AutomationProperties.SetName(placeholder, _resources.GetString("MarkdownPreview_Loading"));
            PreviewLoading.Child = placeholder;
        }
        else if (!loading)
        {
            PreviewLoading.Child = null;
        }
        PreviewLoading.Visibility = loading ? Visibility.Visible : Visibility.Collapsed;
    }

    private void ShowStatus(string key)
    {
        PreviewStatus.Text = key == null ? string.Empty : _resources.GetString(key);
        PreviewStatus.Visibility = key == null ? Visibility.Collapsed : Visibility.Visible;
    }

}
