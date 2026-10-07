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

internal static class SessionControllerTests
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
                    "A new scope restored a tab."), "session.prepare", "session.metadata", "session.catalog");
                var kept = await window.Core.OpenAsync("kept\r");
                var closing = await window.Core.OpenAsync("closing\r");
                kept.Document.Editor.InsertText(0, "typed\r");
                window.Core.TabScrollOffset = 17;
                keptId = kept.Id;
                var saved = false;
                await observer.ExpectOffUIAsync(async () => SessionTestProtocol.Check(
                    await controller.SaveSessionAsync(() => saved = true) && saved,
                    "The controller did not back up its tabs or skipped the after-save action."),
                    "session.metadata", "session.journal.flush", "session.publish");
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
                await observer.ExpectOffUIAsync(async () => SessionTestProtocol.Check(await controller.LoadLastSessionAsync() == 1,
                    "The controller did not restore exactly the kept tab."), "session.prepare", "session.metadata", "session.publish");
                var restored = (FakeTextEditor)window.Core.GetAllTextEditors().Single();
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
        log.AppendLine("PASS: the session controller runs authority, restore preparation, attach commit, backup, explicit close, maintenance and clear on the pool inside its serialized steps, while capture and restore stay on the UI thread across two window lifetimes.");
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

        public ControllerHost(Guid owner, SessionRegistry registry)
        {
            var dispatcher = Window.Current.Dispatcher;
            Core = new FakeNotepadsCore(owner, dispatcher);
            Service = SessionTestProtocol.CreateService(owner);
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
        private readonly List<FakeTextEditor> _open = [];

        public FakeNotepadsCore(Guid owner, CoreDispatcher dispatcher) { _owner = owner; _dispatcher = dispatcher; }

        public event EventHandler<ITextEditor> TextEditorOpened;
        public event EventHandler<ITextEditor> TextEditorClosed;
#pragma warning disable CS0067
        public event EventHandler<ITextEditor> TextEditorLoaded, TextEditorLanguageChanged,
            TextEditorEditorModificationStateChanged, TextEditorFileModificationStateChanged, TextEditorSaved, TextEditorClosing,
            TextEditorRenamed, TextEditorSelectionChanged, TextEditorFontZoomFactorChanged, TextEditorEncodingChanged,
            TextEditorLineEndingChanged, TextEditorMovedToAnotherAppInstance;
        public event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;
        public event KeyEventHandler TextEditorKeyDown;
#pragma warning restore CS0067

        public Guid DocumentOwnerId => _owner;
        public bool IsClosing { get; set; }
        public double TabScrollOffset { get; set; }

        public async Task<FakeTextEditor> OpenAsync(string text)
        {
            var editor = (FakeTextEditor)CreateTextEditor(Guid.NewGuid(), null, null);
            await editor.CreateDocumentAsync(text);
            OpenTextEditors(new ITextEditor[] { editor });
            return editor;
        }

        public ITextEditor CreateTextEditor(Guid id, StorageFile editingFile, string fileNamePlaceHolder)
        {
            RequireUIThread(_dispatcher);
            var editor = new FakeTextEditor(id, _owner, _dispatcher);
            _created.Add(editor);
            return editor;
        }

        public void OpenTextEditors(ITextEditor[] editors, Guid? selectedEditorId = null)
        {
            RequireUIThread(_dispatcher);
            foreach (FakeTextEditor editor in editors)
            {
                _open.Add(editor);
                TextEditorOpened?.Invoke(this, editor);
            }
        }

        public void Close(FakeTextEditor editor)
        {
            _open.Remove(editor);
            TextEditorClosed?.Invoke(this, editor);
        }

        public ITextEditor[] GetAllTextEditors() { RequireUIThread(_dispatcher); return _open.ToArray(); }
        public ITextEditor GetSelectedTextEditor() { RequireUIThread(_dispatcher); return _open.FirstOrDefault(); }
        public int GetNumberOfOpenedTextEditors() => _open.Count;
        public double GetTabScrollViewerHorizontalOffset() { RequireUIThread(_dispatcher); return TabScrollOffset; }
        public void SetTabScrollViewerHorizontalOffset(double offset) { RequireUIThread(_dispatcher); TabScrollOffset = offset; }

        public async ValueTask DisposeAsync()
        {
            foreach (var editor in _created) await editor.ReleaseDocumentAsync();
        }

        public void Dispose() { }

        public Task<ITextEditor> CreateTextEditorAsync(Guid id, StorageFile file, Encoding encoding) => throw Unused();
        public Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder) => throw Unused();
        public void OpenTextEditor(ITextEditor editor, int atIndex) => throw Unused();
        public Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file) => throw Unused();
        public void DeleteTextEditor(ITextEditor textEditor) => throw Unused();
        public bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content) => throw Unused();
        public bool HaveUnsavedTextEditor() => throw Unused();
        public bool HaveNonemptyTextEditor() => throw Unused();
        public void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding) => throw Unused();
        public void SwitchTo(bool next) => throw Unused();
        public void SwitchTo(int index) => throw Unused();
        public void SwitchTo(ITextEditor textEditor) => throw Unused();
        public ITextEditor GetTextEditor(StorageFile file) => throw Unused();
        public void FocusOnTextEditor(ITextEditor textEditor) => throw Unused();
        public void FocusOnSelectedTextEditor() => throw Unused();
        public void CloseTextEditor(ITextEditor textEditor) => throw Unused();
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
            Document = await SessionTestDocument.FromPreparedAsync(_owner, new PreparedRecoveryDocument
            { SavedSnapshot = savedSnapshot, RecoveryBaseline = recoveryBaseline, Checkpoint = checkpoint, Metadata = metadata });
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
        public Task RestoreAsync(DocumentSnapshot snapshot, StorageFile file, DocumentMetadata metadata,
            DocumentBaseline pendingBaseline) => throw Unused();
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
