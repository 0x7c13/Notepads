// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Threading.Tasks;
using Notepads.Markdown;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Controls.TextEditor;
using EditorHost = Notepads.Presentation.Controls.TextEditor.TextEditor;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Theming;
using Notepads.Presentation.Workspace;
using Windows.ApplicationModel.Resources;
using Windows.Storage;
using Windows.UI;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView : UserControl, IDisposable
{
    private const double PreviewPadding = 20;
    private readonly ResourceLoader _resources = ResourceLoader.GetForCurrentView();
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "The host owns this editor and waits for DisposalCompletion before disposing its native core.")]
    private EditorHost _owner;
    [SuppressMessage("Usage", "CA2213:Disposable fields should be disposed",
        Justification = "This core is borrowed from the host; the preview owns only its source session.")]
    private TextEditorCore _editor;
    private MarkdownSourceSession _session;
    private MarkdownImageProvider _images;
    private StorageFile _resourceFile;
    private string _resourcePath;
    private bool _followSource = true;
    private Task _retirement = Task.CompletedTask;
    private Task _disposalCompletion = Task.CompletedTask;
    private bool _enabled, _active, _disposed, _suspendedForExplicitReopen;
    private ulong _providerGeneration;
    private ulong _linkRequest;
    private FragmentRequest? _pendingFragment;
    private MarkdownView _view => PreviewDocument;

    internal MarkdownView PreviewView => _view;
    internal MarkdownSourceSession SourceSession => _session;
    public Task DisposalCompletion => _disposalCompletion;
    internal Task SuspensionCompletion => Task.WhenAll(_retirement, SourceLinkCompletion,
        _session?.Completion ?? Task.CompletedTask);

    internal bool TrySuspendForExplicitReopen()
    {
        // Incomplete/failed source admissions take the ordinary terminal close.
        // No hidden worker or provider stays live just to populate this cache.
        if (_disposed || !_enabled || !_active || _session?.Status != MarkdownSessionStatus.Ready ||
            !_session.IsCurrent) return false;
        _suspendedForExplicitReopen = true;
        _enabled = false;
        StopActive(preserveDocument: true);
        FinishSessionViewportRestore();
        _view.Visibility = Visibility.Collapsed;
        _view.SuspendCachedDocument();
        return _view.HasCachedDocument;
    }

    internal void PrepareExplicitReopen()
    {
        if (_disposed) throw new ObjectDisposedException(nameof(MarkdownExtensionView));
        ApplyFrontMatterStyle();
        FollowSource = true;
        // An explicit reopen follows the current source viewport, rather than
        // the preserved reading position used when returning to a cached tab.
        BeginSessionViewportRestore();
        _view.Visibility = Visibility.Visible;
    }

    internal bool CanResumeExplicitReopen()
    {
        if (_disposed || _enabled || _active) return false;
        // A settings notification may still be queued from another UI thread.
        ApplyFrontMatterStyle();
        return _session.CanResumeCachedDocument;
    }

    internal void DiscardRetainedDocument() => _view.DiscardCachedDocument();

    internal bool FollowSource
    {
        get => _followSource;
        set
        {
            if (_followSource == value) return;
            _followSource = value;
            _view.ShowInactiveCaretMarker = value;
            ReleasePreservedViewport();
            if (!value) FinishSessionViewportRestore();
            if (_session != null && !_disposed) SourceLinkStateChanged();
        }
    }

    public bool IsExtensionEnabled
    {
        get => _enabled;
        set
        {
            if (_disposed) return;
            _enabled = value;
            _suspendedForExplicitReopen = false;
            UpdateActive();
        }
    }

    internal void SetCachedPresentationVisible(bool visible)
    {
        if (_disposed) return;
        if (!visible)
        {
            StopViewportRestoreDeadline();
            CancelPendingFragment();
            RetireNoteEmbeds();
        }
        // Mark host retention explicitly without detaching the native view.
        // Only real image pressure may suspend its retained presentation.
        _view.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        if (visible) UpdateViewportRestoreDeadline();
        if (visible) _session?.Wake();
        if (!visible || _disposed || !_enabled || !_active || _session == null ||
            _view.Visibility != Visibility.Visible) return;
        if (!_view.IsCachedPresentationSuspended)
        {
            RefreshNoteEmbeds();
            return;
        }
        var session = _session;
        session.Stop(preserveDocument: true);
        // Visibility and Changed callbacks can reenter terminal close.
        if (_disposed || !_enabled || !_active || !ReferenceEquals(_session, session) ||
            _view.Visibility != Visibility.Visible) return;
        session.Start();
    }

    public MarkdownExtensionView()
    {
        _linkSourcePointerHandler = LinkSourcePointer;
        _linkSourceWheelHandler = LinkSourceWheel;
        _linkPreviewPointerHandler = LinkPreviewPointer;
        _linkPreviewWheelHandler = LinkPreviewWheel;
        InitializeComponent();
        _view.CacheDocumentOnUnload = true;
        _view.ShowInactiveCaretMarker = FollowSource;
        _view.SetScrollSettingsProvider(new MarkdownScrollSettingsProvider());
        AutomationProperties.SetName(_view, _resources.GetString("TextEditor_ContextFlyout_PreviewToggleDisplay_Text"));
        Loaded += ViewLoaded;
        Unloaded += ViewUnloaded;
        ActualThemeChanged += ViewThemeChanged;
        _view.LinkRequested += LinkRequested;
        _view.CodeCopied += PreviewCodeCopied;
        _view.InitialViewportChanged += InitialViewportChanged;
        _view.ViewportChanged += PendingFragmentViewportChanged;
        _view.VerticalScrollChanged += PendingFragmentScrollChanged;
        _accessibility.HighContrastChanged += HighContrastChanged;
        ThemeSettingsService.OnAccentColorChanged += AccentChanged;
    }

    private void PreviewCodeCopied(MarkdownView sender, object args)
    {
        if (_active && !_disposed && _view.Visibility == Visibility.Visible)
            NotificationCenter.Instance.PostNotification(_resources.GetString("Clipboard_NotificationMsg_Copied"), 1500);
    }

    public void Bind(EditorHost editor)
    {
        ArgumentNullException.ThrowIfNull(editor);
        if (_disposed || _owner != null) throw new InvalidOperationException("A preview can bind to one live editor.");
        _owner = editor;
        _editor = editor.MarkdownCore;
        _session = new MarkdownSourceSession(_editor, _view);
        ApplyFrontMatterStyle();
        ApplicationPreferences.MarkdownFrontMatterStyleChanged += FrontMatterStyleChanged;
        _session.Changed += SessionChanged;
        _session.EditAccepted += ForwardDraftNoteEdit;
        _editor.FontSizeChanged += EditorFontChanged;
        _fontFamilyCallback = _editor.RegisterPropertyChangedCallback(TextEditorCore.FontFamilyProperty, EditorFontFamilyChanged);
        editor.FileSaved += ResourceFileSaved;
        editor.FileRenamed += ResourceContextChanged;
        editor.FileReloaded += ResourceContextChanged;
        InitializeSourceLink();
        UpdateActive();
    }

    private void ApplyFrontMatterStyle() => _session?.SetFrontMatterStyle(ApplicationPreferences.MarkdownFrontMatterView);

    private async void FrontMatterStyleChanged(object sender, EventArgs args)
    {
        if (_disposed) return;
        if (Dispatcher.HasThreadAccess)
        {
            ApplyFrontMatterStyle();
            return;
        }
        try
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                if (!_disposed) ApplyFrontMatterStyle();
            });
        }
        catch (Exception error) { LoggingService.LogException(error); }
    }

    private void ViewLoaded(object sender, RoutedEventArgs args) => UpdateActive();
    private void ViewUnloaded(object sender, RoutedEventArgs args)
    {
        StopActive(preserveDocument: true);
        if (_disposed || !_enabled) return;

        // A tab can be selected again before XAML delivers the old Unloaded.
        // Reconcile after layout so that stale notification cannot stop a
        // preview that has already reattached and become visible.
        _ = Dispatcher.TryRunAsync(CoreDispatcherPriority.Low, () =>
        {
            if (!_disposed && _view.IsLoaded && _view.Visibility == Visibility.Visible)
                UpdateActive();
        });
    }

    private void UpdateActive()
    {
        if (_disposed || !_enabled || _session == null)
        {
            // Attaching the cached XAML owner can raise Loaded before the host
            // enables it. That intermediate notification must preserve its pin.
            StopActive(preserveDocument: _suspendedForExplicitReopen && !_disposed);
            return;
        }
        if (!IsLoaded) return;
        if (_active) return;
        _active = true;
        try
        {
            ReplaceImages();
            ApplyPreviewTheme();
            _session.Start();
            StartSourceLink();
            ApplyPendingFragment();
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            StopActive();
            ShowStatus("MarkdownPreview_Failed");
        }
    }

    private void StopActive(bool preserveDocument = false)
    {
        if (_active || !preserveDocument) _pendingFragment = null;
        if (!_active)
        {
            if (!preserveDocument)
            {
                _session?.Stop();
                FinishSessionViewportRestore();
            }
            StopViewportResourceRelease();
            if (!preserveDocument || !_restoringSessionViewport) _view.IsResourceRenderingDeferred = false;
            return;
        }
        _active = false;
        RetireNoteEmbeds();
        StopViewportRestoreDeadline();
        StopSourceLink();
        _session.Stop(preserveDocument);
        RetireImages();
        _retirement = Task.WhenAll(_retirement, _session.Completion, SourceLinkCompletion);
        if (!preserveDocument) FinishSessionViewportRestore();
        StopViewportResourceRelease();
        if (!preserveDocument || !_restoringSessionViewport) _view.IsResourceRenderingDeferred = false;
    }

    private void RetireImages()
    {
        var previous = _images;
        _images = null;
        _resourceFile = null;
        _resourcePath = null;
        try { _view.SetImageProvider(null, checked(++_providerGeneration)); }
        finally
        {
            if (previous != null)
            {
                previous.Dispose();
                _retirement = Task.WhenAll(_retirement, previous.Completion);
            }
        }
    }

    private void ReplaceImages()
    {
        RetireImages();
        var file = _owner.EditingFile;
        var path = file?.Path;
        var provider = new MarkdownImageProvider(file);
        try
        {
            _images = provider;
            _view.SetImageProvider(provider, checked(++_providerGeneration));
            _resourceFile = file;
            _resourcePath = path;
        }
        catch
        {
            _images = null;
            provider.Dispose();
            _retirement = Task.WhenAll(_retirement, provider.Completion);
            throw;
        }
    }

    private void ResourceFileSaved(object sender, EventArgs args)
    {
        if (!_active || _disposed) return;
        var file = _owner.EditingFile;
        var path = file?.Path;
        if (_images != null && (string.IsNullOrEmpty(path)
                ? string.IsNullOrEmpty(_resourcePath) && ReferenceEquals(file, _resourceFile)
                : string.Equals(path, _resourcePath, StringComparison.Ordinal))) return;
        ResourceContextChanged(sender, args);
    }

    private void ResourceContextChanged(object sender, EventArgs args)
    {
        if (!_active || _disposed) return;
        try
        {
            ReplaceImages();
            RetireNoteEmbeds();
            RefreshNoteEmbeds();
        }
        catch (Exception error) { LoggingService.LogException(error); ShowStatus("MarkdownPreview_Failed"); }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _enabled = false;
        StopActive();
        DisposeSourceLink();
        _view.SetScrollSettingsProvider(null);
        if (_session != null)
        {
            _session.Changed -= SessionChanged;
            _session.EditAccepted -= ForwardDraftNoteEdit;
            _session.Dispose();
        }
        if (_editor != null)
        {
            _editor.FontSizeChanged -= EditorFontChanged;
            if (_fontFamilyCallback is { } callback)
            {
                _editor.UnregisterPropertyChangedCallback(TextEditorCore.FontFamilyProperty, callback);
                _fontFamilyCallback = null;
            }
        }
        if (_owner != null)
        {
            _owner.FileSaved -= ResourceFileSaved;
            _owner.FileRenamed -= ResourceContextChanged;
            _owner.FileReloaded -= ResourceContextChanged;
        }
        Loaded -= ViewLoaded;
        Unloaded -= ViewUnloaded;
        ActualThemeChanged -= ViewThemeChanged;
        _view.LinkRequested -= LinkRequested;
        _view.CodeCopied -= PreviewCodeCopied;
        _view.InitialViewportChanged -= InitialViewportChanged;
        _view.ViewportChanged -= PendingFragmentViewportChanged;
        _view.VerticalScrollChanged -= PendingFragmentScrollChanged;
        _accessibility.HighContrastChanged -= HighContrastChanged;
        ThemeSettingsService.OnAccentColorChanged -= AccentChanged;
        ApplicationPreferences.MarkdownFrontMatterStyleChanged -= FrontMatterStyleChanged;
        _disposalCompletion = Task.WhenAll(_retirement, SourceLinkCompletion, _session?.Completion ?? Task.CompletedTask);
    }
}
