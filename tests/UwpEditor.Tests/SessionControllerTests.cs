// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics.Metrics;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Workspace;
using Windows.Storage;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Input;
using WinUIEditor;

namespace NotepadsEditorTests;

internal static partial class SessionControllerTests
{
    public static async Task RunAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        var registry = new SessionRegistry();
        var observer = new PersistenceThreadObserver(Environment.CurrentManagedThreadId);
        try
        {
            Guid keptId;
            // First window: bind authority, restore nothing, back up two tabs, close one durably, then maintain.
            await using (var window = new ControllerHost(owner, registry))
            {
                var controller = window.Controller;
                await observer.ExpectOffUIAsync(controller.InitializeAuthorityAsync, "session.catalog");
                await observer.ExpectOffUIAsync(async () => SessionTestProtocol.Check(await controller.LoadLastSessionAsync() == 0,
                    "A new scope restored a tab."), "session.discover", "session.catalog");
                var kept = await window.Core.OpenAsync("kept\r");
                var closing = await window.Core.OpenAsync("closing\r");
                kept.Document.Editor.InsertText(0, "typed\r");
                window.Core.TabScrollOffset = 17;
                keptId = kept.Id;
                var saved = false;
                await observer.ExpectOffUIAsync(async () => SessionTestProtocol.Check(
                    await controller.SaveSessionAsync(() => saved = true) && saved,
                    "The controller did not back up its tabs or skipped the after-save action."),
                    "session.journal.flush", "session.publish");
                var closed = new HashSet<Guid>();
                await observer.ExpectOffUIAsync(async () => SessionTestProtocol.Check(
                    await controller.PrepareExplicitCloseAsync(new[] { closing.Id }, closed) && closed.SetEquals(new[] { closing.Id }),
                    "The controller did not record a durable tab close."), "session.catalog", "notepads.session.metadata.writes");
                window.Core.Close(closing);
                await observer.ExpectOffUIAsync(controller.RunStartupMaintenanceAsync, "session.maintenance");
            }

            // Second window on the same scope: the kept tab restores on the UI thread from assets prepared on the pool.
            await using (var window = new ControllerHost(owner, registry))
            {
                var controller = window.Controller;
                await observer.ExpectOffUIAsync(controller.InitializeAuthorityAsync, "session.catalog");
                await observer.ExpectOffUIAsync(async () =>
                {
                    SessionTestProtocol.Check(await controller.LoadLastSessionAsync() == 1,
                        "The controller did not restore exactly the kept tab.");
                    await controller.WaitForRestorationCheckpointAsync();
                }, "session.discover", "session.publish");
                var restored = (FakeTextEditor)window.Core.GetTabs().Single().ReadyEditor;
                SessionTestProtocol.Check(restored.Id == keptId && restored.Text == "typed\rkept\r" && window.Core.TabScrollOffset == 17,
                    "The restored tab lost its identity, its journaled text or the tab strip offset.");
                var epoch = window.Service.CurrentStamp.EpochId;
                await observer.ExpectOffUIAsync(controller.ClearSessionDataAsync, "session.catalog", "notepads.session.metadata.writes");
                SessionTestProtocol.Check(window.Service.CurrentStamp.EpochId != epoch, "Clearing session data kept the recovery epoch.");
            }
        }
        finally
        {
            // Cleanup reads recovery records on the UI thread itself.
            observer.Dispose();
            await SessionTestProtocol.CleanupAsync(owner);
        }
        log.AppendLine("PASS: session authority, discovery, background preparation, backup, close, maintenance and reset run on the pool; native restore and capture stay on the UI thread across two window lifetimes.");
        await VerifyLazyWorkspaceAsync(log);
        await VerifyInterruptedResetAsync(log);
        await VerifyBackgroundOutcomesAsync(log);
        await VerifyImportedWorkspaceAsync(log);
    }

    private static NotSupportedException Unused() => new("The session controller test host does not use this member.");

    private static void RequireUIThread(CoreDispatcher dispatcher)
    {
        if (!dispatcher.HasThreadAccess) throw new InvalidOperationException("Editor state was accessed off the UI thread.");
    }

    /// <summary>Observes recovery metrics, which record on the thread doing the persistence work.</summary>
    private sealed class PersistenceThreadObserver : IDisposable
    {
        private readonly MeterListener _listener = new();
        private readonly ConcurrentQueue<(string Name, bool OnUIThread)> _observations = new();
        private readonly int _uiThread;

        public PersistenceThreadObserver(int uiThread)
        {
            _uiThread = uiThread;
            _listener.InstrumentPublished = (instrument, listener) =>
            {
                if (instrument.Meter.Name == OperationMetrics.MeterName) listener.EnableMeasurementEvents(instrument);
            };
            _listener.SetMeasurementEventCallback<long>((instrument, value, tags, state) => Record(instrument.Name));
            _listener.SetMeasurementEventCallback<double>((instrument, value, tags, state) =>
            {
                foreach (var tag in tags)
                    if (tag.Key == "operation") Record(tag.Value as string);
            });
            _listener.Start();
        }

        private void Record(string name) => _observations.Enqueue((name, Environment.CurrentManagedThreadId == _uiThread));

        public async Task ExpectOffUIAsync(Func<Task> step, params string[] expected)
        {
            _observations.Clear();
            await step();
            var observed = _observations.ToArray();
            var onUIThread = observed.Where(item => item.OnUIThread).Select(item => item.Name).Distinct().ToArray();
            SessionTestProtocol.Check(onUIThread.Length == 0, $"Session persistence ran on the UI thread: {string.Join(", ", onUIThread)}.");
            var missing = expected.Except(observed.Select(item => item.Name)).ToArray();
            SessionTestProtocol.Check(missing.Length == 0, $"A controller step did not reach its persistence work: {string.Join(", ", missing)}.");
        }

        public void Dispose() => _listener.Dispose();
    }

    /// <summary>One window's session wiring, closed in OnWindowClosed's order.</summary>
    private sealed class ControllerHost : IAsyncDisposable
    {
        private bool _closed;

        public ControllerHost(Guid owner, SessionRegistry registry, RecoveryRecordStore store = null)
        {
            var dispatcher = Window.Current.Dispatcher;
            Core = new FakeNotepadsCore(owner, dispatcher);
            Service = SessionTestProtocol.CreateService(owner, store);
            Controller = new SessionController(Core, Service, dispatcher, registry.Register);
        }

        public FakeNotepadsCore Core { get; }
        public SessionService Service { get; }
        public SessionController Controller { get; }

        public async ValueTask DisposeAsync()
        {
            if (_closed) return;
            _closed = true;
            // Drain the controller, release the editors, then the scope's writer lease.
            Controller.Dispose();
            try { await Controller.DrainAsync(); }
            finally
            {
                try { await Core.DisposeAsync(); }
                finally { await Service.DisposeAsync(); }
            }
        }
    }

    /// <summary>A tab strip without XAML: it opens, closes and lists fake editors, on the UI thread only.</summary>
    private sealed class FakeNotepadsCore : INotepadsCore
    {
        private readonly Guid _owner;
        private readonly CoreDispatcher _dispatcher;
        private readonly List<FakeTextEditor> _created = [];
        private readonly List<WorkspaceTab> _open = [];
        private Guid? _selectedEditorId;

        public FakeNotepadsCore(Guid owner, CoreDispatcher dispatcher) { _owner = owner; _dispatcher = dispatcher; }

        public event EventHandler<WorkspaceTab> TabOpened, TabClosed, TabSelected, EditorAttached;
#pragma warning disable CS0067
        public event EventHandler<WorkspaceTab> TabClosing, TabStateChanged;
        public event EventHandler<ITextEditor> TextEditorLoaded, TextEditorLanguageChanged,
            TextEditorEditorModificationStateChanged, TextEditorFileModificationStateChanged, TextEditorSaved,
            TextEditorRenamed, TextEditorSelectionChanged, TextEditorFontZoomFactorChanged, TextEditorEncodingChanged,
            TextEditorLineEndingChanged, TextEditorMovedToAnotherAppInstance;
        public event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;
        public event KeyEventHandler TextEditorKeyDown;
#pragma warning restore CS0067

        public Guid DocumentOwnerId => _owner;
        public bool IsClosing { get; set; }
        public bool IsEditorPresentationPaused { get; set; }
        public double TabScrollOffset { get; set; }
        public Action<Guid> RestoreStarting { get; set; }
        public Action<Guid> NativeRestoreReturned { get; set; }
        public Action<FakeTextEditor> RestoreReady { get; set; }
        public Action SelectedAttached { get; set; }

        public async Task<FakeTextEditor> OpenAsync(string text)
        {
            var editor = (FakeTextEditor)CreateTextEditor(Guid.NewGuid(), null, null);
            await editor.CreateDocumentAsync(text);
            OpenTextEditor(editor);
            return editor;
        }

        public ITextEditor CreateTextEditor(Guid id, StorageFile editingFile, string fileNamePlaceHolder, bool allowWhileClosing = false)
        {
            RequireUIThread(_dispatcher);
            var editor = new FakeTextEditor(id, _owner, _dispatcher)
            {
                RestoreStarting = RestoreStarting,
                NativeRestoreReturned = NativeRestoreReturned,
                RestoreReady = RestoreReady
            };
            _created.Add(editor);
            return editor;
        }

        public void OpenTextEditor(ITextEditor editor, int atIndex = -1) => OpenTab(new WorkspaceTab(editor), atIndex);

        public void OpenTab(WorkspaceTab tab, int atIndex = -1, bool select = true, bool allowWhileClosing = false)
        {
            RequireUIThread(_dispatcher);
            if (atIndex < 0) _open.Add(tab);
            else _open.Insert(atIndex, tab);
            TabOpened?.Invoke(this, tab);
            if (tab.ReadyEditor != null) EditorAttached?.Invoke(this, tab);
            if (select) SwitchTo(tab);
        }

        public void OpenTabs(WorkspaceTab[] tabs, Guid? selectedTabId = null, bool allowWhileClosing = false)
        {
            foreach (var tab in tabs) OpenTab(tab, select: false);
            SwitchTo(tabs.FirstOrDefault(tab => tab.Id == selectedTabId) ?? tabs.LastOrDefault());
        }

        public bool AttachEditor(WorkspaceTab tab, ITextEditor editor, bool showEditor = true)
        {
            RequireUIThread(_dispatcher);
            if (!ContainsTab(tab) || !tab.TrySetReady(editor)) return false;
            EditorAttached?.Invoke(this, tab);
            tab.CompleteLoading();
            if (showEditor && GetSelectedTab() == tab) SelectedAttached?.Invoke();
            return true;
        }

        public void ResumeEditorPresentation() { }
        public async Task DiscardUnattachedEditorAsync(ITextEditor editor)
        {
            if (GetTab(editor) != null) throw new InvalidOperationException("Cannot discard an attached editor.");
            _created.Remove((FakeTextEditor)editor);
            editor.Dispose();
            await ((FakeTextEditor)editor).ReleaseDocumentAsync();
            await editor.DisposalCompletion;
        }
        public void SwitchTo(WorkspaceTab tab)
        {
            RequireUIThread(_dispatcher);
            _selectedEditorId = tab?.Id;
            TabSelected?.Invoke(this, tab);
        }

        public void Close(FakeTextEditor editor) => CloseTab(GetTab(editor));
        public void DeleteTab(WorkspaceTab tab) => CloseTab(tab);
        public void CloseTab(WorkspaceTab tab)
        {
            RequireUIThread(_dispatcher);
            tab.MarkClosed();
            _open.Remove(tab);
            if (_selectedEditorId == tab.Id) SwitchTo(_open.LastOrDefault());
            TabClosed?.Invoke(this, tab);
        }

        public WorkspaceTab[] GetTabs() { RequireUIThread(_dispatcher); return _open.ToArray(); }
        public WorkspaceTab GetSelectedTab() { RequireUIThread(_dispatcher); return _open.FirstOrDefault(tab => tab.Id == _selectedEditorId); }
        public ITextEditor GetSelectedReadyEditor() => GetSelectedTab()?.ReadyEditor;
        public WorkspaceTab GetTab(ITextEditor editor) => _open.FirstOrDefault(tab => tab.AttachedEditor == editor);
        public bool ContainsTab(WorkspaceTab tab) => _open.Contains(tab);
        public int GetTabCount() => _open.Count;
        public double GetTabScrollViewerHorizontalOffset() { RequireUIThread(_dispatcher); return TabScrollOffset; }
        public void SetTabScrollViewerHorizontalOffset(double offset) { RequireUIThread(_dispatcher); TabScrollOffset = offset; }

        public async ValueTask DisposeAsync()
        {
            foreach (var editor in _created) await editor.ReleaseDocumentAsync();
        }

        public void Dispose() { }

        public Task<ITextEditor> CreateTextEditorAsync(Guid id, StorageFile file, Encoding encoding) => throw Unused();
        public Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder) => throw Unused();
        public Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file) => throw Unused();
        public bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content) => throw Unused();
        public bool HasTabsRequiringSave() => throw Unused();
        public bool HasPrintableTabs() => throw Unused();
        public void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding) => throw Unused();
        public void SwitchTo(bool next) => throw Unused();
        public void SwitchTo(int index) => throw Unused();
        public void SwitchTo(ITextEditor textEditor) => throw Unused();
        public Task<WorkspaceTab> FindTabAsync(StorageFile file) => throw Unused();
        public void FocusOnTextEditor(ITextEditor textEditor) => throw Unused();
        public void FocusOnSelectedTextEditor() => throw Unused();
    }

    /// <summary>A native document with a journal standing in for the XAML editor's recovery surface.</summary>
    private sealed class FakeTextEditor : ITextEditor
    {
        private readonly Guid _owner;
        private readonly CoreDispatcher _dispatcher;

        public FakeTextEditor(Guid id, Guid owner, CoreDispatcher dispatcher) { Id = id; _owner = owner; _dispatcher = dispatcher; }

#pragma warning disable CS0067
        public event RoutedEventHandler Loaded;
        public event KeyEventHandler KeyDown;
        public event EventHandler ModificationStateChanged, FileModificationStateChanged, LineEndingChanged,
            EncodingChanged, SelectionChanged, FontZoomFactorChanged, TextChanging, ChangeReverted, FileSaved, FileReloaded,
            FileRenamed, LanguageChanged, LanguageOverrideChanged;
#pragma warning restore CS0067

        public SessionTestDocument Document { get; private set; }
        public Action<Guid> RestoreStarting { get; set; }
        public Action<Guid> NativeRestoreReturned { get; set; }
        public Action<FakeTextEditor> RestoreReady { get; set; }
        public string Text => Document.Editor.GetText(Document.Editor.Length);
        public Guid Id { get; set; }
        public StorageFile EditingFile => null;
        public string EditingFileName => null;
        public string EditingFilePath => null;
        public Task DisposalCompletion => Task.CompletedTask;

        public async Task CreateDocumentAsync(string text) => Document = await SessionTestDocument.CreateAsync(_owner, text);

        public DocumentRecoveryState CaptureRecoveryState()
        {
            RequireUIThread(_dispatcher);
            return Document.CaptureState();
        }

        public Task RepairRecoveryJournalAsync(CancellationToken cancellationToken)
        {
            RequireUIThread(_dispatcher);
            return Task.CompletedTask;
        }

        // As TextEditor does natively: replay the prepared checkpoint, then fork a journal this scope owns.
        public async Task RestoreV2Async(DocumentSnapshot savedSnapshot, DocumentBaseline recoveryBaseline, DocumentJournal journal,
            EditorJournalCheckpoint checkpoint, StorageFile file, DocumentMetadata metadata, bool textDirty)
        {
            RequireUIThread(_dispatcher);
            RestoreStarting?.Invoke(Id);
            Document = await SessionTestDocument.FromPreparedAsync(_owner, new PreparedRecoveryDocument
            { SavedSnapshot = savedSnapshot, RecoveryBaseline = recoveryBaseline, Checkpoint = checkpoint, Metadata = metadata },
                () => NativeRestoreReturned?.Invoke(Id));
            RestoreReady?.Invoke(this);
        }

        public void Dispose() => RequireUIThread(_dispatcher);

        public async Task ReleaseDocumentAsync()
        {
            if (Document != null) await Document.DisposeAsync();
            Document = null;
        }

        public DocumentLanguage DocumentLanguage => throw Unused();
        public DocumentLanguage DetectedLanguage => throw Unused();
        public string LanguageOverride => throw Unused();
        public EditorSyntaxPauseReason SyntaxHighlightingPauseReason => throw Unused();
        public bool CanChangeLanguage => throw Unused();
        public void SetLanguageOverride(string id) => throw Unused();
        public FileType FileType => throw Unused();
        public DocumentSnapshot LastSavedSnapshot => throw Unused();
        public Guid DocumentOwnerId { get => throw Unused(); set => throw Unused(); }
        public LineEnding? RequestedLineEnding => throw Unused();
        public Encoding RequestedEncoding => throw Unused();
        public string FileNamePlaceholder { get => throw Unused(); set => throw Unused(); }
        public bool IsModified => throw Unused();
        public bool IsDocumentEmpty => throw Unused();
        public long DocumentLength => throw Unused();
        public FileModificationState FileModificationState => throw Unused();
        public TextEditorMode Mode => throw Unused();
        public bool DisplayLineNumbers { get => throw Unused(); set => throw Unused(); }
        public bool DisplayLineHighlighter { get => throw Unused(); set => throw Unused(); }
        public ulong DocumentSequence => throw Unused();
        public void InitEmpty(Encoding encoding, LineEnding lineEnding, StorageFile file) => throw Unused();
        public Task InitializeNewAsync() => throw Unused();
        public Task MaintainRecoveryAsync(CancellationToken cancellationToken) => throw Unused();
        public Task WaitForDocumentOperationsAsync() => throw Unused();
        public Task<bool> TryCommitTransferAsync(Func<bool> sourceUnchanged, Func<Task> commitRemovalIntent,
            Action removeLogicalSource) => throw Unused();
        public Task InitAsync(DocumentSnapshot snapshot, StorageFile file, bool isModified) => throw Unused();
        public async Task RestoreAsync(DocumentSnapshot snapshot, StorageFile file, DocumentMetadata metadata,
            DocumentBaseline pendingBaseline)
        {
            RequireUIThread(_dispatcher);
            if (pendingBaseline != null || file != null) throw Unused(); // Saved-only legacy import fixture.
            RestoreStarting?.Invoke(Id);
            Document = await SessionTestDocument.CreateAsync(_owner, snapshot.Baseline);
            Document.Metadata = metadata.Clone();
            NativeRestoreReturned?.Invoke(Id);
            RestoreReady?.Invoke(this);
        }
        public Task RenameAsync(string newFileName, StorageFile expectedFile) => throw Unused();
        public string GetText() => throw Unused();
        public void StartCheckingFileStatusPeriodically() => throw Unused();
        public void StopCheckingFileStatus() => throw Unused();
        public DocumentMetadata GetTextEditorStateMetaData() => throw Unused();
        public void ResetEditorState(DocumentMetadata metadata) => throw Unused();
        public Task ReloadFromEditingFileAsync(Encoding encoding, DocumentDecodingMode decodingMode) => throw Unused();
        public LineEnding GetLineEnding() => throw Unused();
        public Encoding GetEncoding() => throw Unused();
        public void CopyTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args) => throw Unused();
        public Task RevertAllChangesAsync() => throw Unused();
        public bool TryChangeEncoding(Encoding encoding) => throw Unused();
        public bool TryChangeLineEnding(LineEnding lineEnding) => throw Unused();
        public void ShowHideContentPreview() => throw Unused();
        public Task OpenSideBySideDiffViewerAsync() => throw Unused();
        public Task ToggleDiffPreviewAsync() => throw Unused();
        public void CloseSideBySideDiffViewer() => throw Unused();
        public void GetLineColumnSelection(out int startLineIndex, out int endLineIndex, out int startColumnIndex,
            out int endColumnIndex, out int selectedCount, out int lineCount) => throw Unused();
        public double GetFontZoomFactor() => throw Unused();
        public void SetFontZoomFactor(double fontZoomFactor) => throw Unused();
        public bool IsEditorEnabled() => throw Unused();
        public Task SaveContentToFileAndUpdateEditorStateAsync(StorageFile file) => throw Unused();
        public string GetContentForSharing() => throw Unused();
        public void TypeText(string text) => throw Unused();
        public void Focus() => throw Unused();
        public bool NoChangesSinceLastSaved() => throw Unused();
        public void ShowFindAndReplaceControl(bool showReplaceBar) => throw Unused();
        public void HideFindAndReplaceControl() => throw Unused();
        public void ShowGoToControl() => throw Unused();
        public void HideGoToControl() => throw Unused();
        public FlyoutBase GetContextFlyout() => throw Unused();
    }
}
