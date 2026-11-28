// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.Markdown;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    private MarkdownExtensionView _markdownPreview;
    private Task _previewShutdown = Task.CompletedTask;
    internal TextEditorCore MarkdownCore => TextEditorCore;
    internal bool CanCacheMarkdownPresentation =>
        !_disposed && _markdownPreview is { IsExtensionEnabled: true } preview &&
        preview.SourceSession != null;
    private bool IsMarkdownPreviewSupported => FileTypeUtility.IsPreviewSupported(FileType, DocumentLanguage?.Id);

    private bool TryHandleMarkdownPreviewKey(MarkdownExtensionView preview, KeyRoutedEventArgs args)
    {
        if (!IsMarkdownPreviewFocused(preview)) return false;
        var window = Window.Current.CoreWindow;
        var modifiers = VirtualKeyModifiers.None;
        if (window.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Control;
        if (window.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Menu;
        if (window.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Shift;
        if (!HandleMarkdownPreviewCommand(args.Key, modifiers)) return false;
        args.Handled = true;
        return true;
    }

    private bool IsMarkdownPreviewFocused(MarkdownExtensionView preview)
    {
        if (_disposed) return false;
        for (var element = FocusManager.GetFocusedElement() as DependencyObject; element != null;
            element = VisualTreeHelper.GetParent(element))
            if (ReferenceEquals(element, preview)) return true;
        return false;
    }

    private bool HandleMarkdownPreviewCommand(VirtualKey key, VirtualKeyModifiers modifiers)
    {
        if (key == VirtualKey.F3 && modifiers is VirtualKeyModifiers.None or VirtualKeyModifiers.Shift)
        {
            if (string.IsNullOrEmpty(_lastSearchContext?.SearchText)) ShowFindAndReplaceControl(false);
            else _ = InitiateFindAndReplaceAsync(new FindAndReplaceEventArgs(_lastSearchContext, string.Empty,
                FindAndReplaceMode.FindOnly, modifiers == VirtualKeyModifiers.Shift ? SearchDirection.Previous : SearchDirection.Next));
            return true;
        }
        // Escape has one shared owner in TextEditor_PreviewKeyDown.
        return key != VirtualKey.Escape && _keyboardCommandHandler.Handle(key, modifiers);
    }

    internal bool TryGetMarkdownScrollSync(out bool enabled)
    {
        enabled = true;
        if (_disposed || Mode != TextEditorMode.Editing || !TextEditorCore.IsEnabled ||
            !IsMarkdownPreviewSupported || !_isContentPreviewPanelOpened ||
            _markdownPreview is not { IsExtensionEnabled: true } preview) return false;
        enabled = preview.FollowSource;
        return true;
    }

    internal void SetMarkdownScrollSync(bool enabled)
    {
        if (TryGetMarkdownScrollSync(out _))
            _markdownPreview.FollowSource = enabled;
    }

    private void ToggleMarkdownPreview()
    {
        if (_isContentPreviewPanelOpened) CloseSplitView(retainMarkdownForReopen: true);
        else SetMarkdownPreviewOpen(true);
    }

    internal bool QueueMarkdownFragment(string fragment, bool wikiHeading = false)
    {
        SetMarkdownPreviewOpen(true);
        if (_markdownPreview is not { IsExtensionEnabled: true } preview) return false;
        preview.QueueFragment(fragment, wikiHeading);
        return true;
    }

    private void SetMarkdownPreviewOpen(bool opened, bool restoreViewport = false)
    {
        if (_disposed) return;
        if (!opened)
        {
            if (_isContentPreviewPanelOpened) CloseSplitView();
            else _markdownReopenCache?.Evict(this);
            return;
        }
        if (_isContentPreviewPanelOpened) return;
        if (!IsMarkdownPreviewSupported || !TextEditorCore.IsEnabled || Mode != TextEditorMode.Editing) return;
        FindName(nameof(SplitPanel));
        FindName(nameof(GridSplitter));
        MarkdownExtensionView extension = null;
        try
        {
            extension = _markdownReopenCache?.Take(this) ?? new MarkdownExtensionView();
            if (extension.SourceSession == null) extension.Bind(this);
            else extension.PrepareExplicitReopen();
            if (restoreViewport) extension.BeginSessionViewportRestore();
            _markdownPreview = extension;
            SplitPanel.Content = extension;
            SplitPanel.Visibility = Visibility.Visible;
            GridSplitter.Visibility = Visibility.Visible;
            EditorColumnDefinition.Width = new GridLength(1, GridUnitType.Star);
            SplitPanelColumnDefinition.Width = new GridLength(1, GridUnitType.Star);
            SplitPanelColumnDefinition.MinWidth = 120;
            _isContentPreviewPanelOpened = true;
            extension.IsExtensionEnabled = true;
        }
        catch
        {
            _markdownReopenCache?.Evict(this);
            if (extension != null) RetireMarkdownPreview(extension, discardDocument: true);
            _markdownPreview = null;
            SplitPanel.Content = null;
            _isContentPreviewPanelOpened = false;
            throw;
        }
    }

    private void StopContentPreview()
    {
        _markdownReopenCache?.Evict(this);
        var extension = _markdownPreview;
        _markdownPreview = null;
        if (extension != null) RetireMarkdownPreview(extension);
        if (SplitPanel != null) SplitPanel.Content = null;
        _isContentPreviewPanelOpened = false;
    }

    private void PreviewLanguageChanged(object sender, EventArgs args)
    {
        if (!IsMarkdownPreviewSupported) _markdownReopenCache?.Evict(this);
        if (_isContentPreviewPanelOpened && !IsMarkdownPreviewSupported) CloseSplitView();
    }
}
