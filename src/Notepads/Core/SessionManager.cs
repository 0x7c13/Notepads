
namespace Notepads.Core
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.Linq;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Newtonsoft.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.Storage;

    internal class SessionManager : ISessionManager
    {
        private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);
        private readonly INotepadsCore _notepadsCore;
        private readonly SemaphoreSlim _semaphoreSlim;
        private readonly ConcurrentDictionary<Guid, TextEditorSessionData> _sessionData;
        private string _lastSessionJsonStr;
        private bool _loaded;
        private Timer _timer;

        /// <summary>
        /// Do not call this constructor directly. Use <see cref="SessionUtility.GetSessionManager(INotepadsCore)" instead./>
        /// </summary>
        public SessionManager(INotepadsCore notepadsCore)
        {
            _notepadsCore = notepadsCore;
            _semaphoreSlim = new SemaphoreSlim(1, 1);
            _sessionData = new ConcurrentDictionary<Guid, TextEditorSessionData>();
            _loaded = false;

            foreach (var editor in _notepadsCore.GetAllTextEditors())
            {
                BindEditorContentStateChangeEvent(this, editor);
            }

            _notepadsCore.TextEditorLoaded += BindEditorContentStateChangeEvent;
            _notepadsCore.TextEditorUnloaded += UnbindEditorContentStateChangeEvent;
        }

        public bool IsBackupEnabled { get; set; }

        public async Task<int> LoadLastSessionAsync()
        {
            if (_loaded)
            {
                return 0; // Already loaded
            }

            var data = await SessionUtility.GetSerializedSessionMetaDataAsync();

            if (data == null)
            {
                return 0; // No session data found
            }

            NotepadsSessionData sessionData;

            try
            {
                sessionData = JsonConvert.DeserializeObject<NotepadsSessionData>(data);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionManager] Failed to load last session metadata: {ex.Message}");
                await ClearSessionDataAsync();
                return 0;
            }

            IList<ITextEditor> recoveredEditor = new List<ITextEditor>();

            foreach (TextEditorSessionData textEditorData in sessionData.TextEditors)
            {
                ITextEditor textEditor;

                try
                {
                    textEditor = await RecoverTextEditorAsync(textEditorData);
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[SessionManager] Failed to recover TextEditor: {ex.Message}");
                    continue;
                }

                if (textEditor != null)
                {
                    recoveredEditor.Add(textEditor);
                    _sessionData.TryAdd(textEditor.Id, textEditorData);
                }
            }

            _notepadsCore.OpenTextEditors(recoveredEditor.ToArray(), sessionData.SelectedTextEditor);
            _notepadsCore.SetSetsViewScrollViewerHorizontalOffset(sessionData.TabScrollViewerHorizontalOffset);

            _loaded = true;

            return _sessionData.Count;
        }

        public async Task SaveSessionAsync(Action actionAfterSaving = null)
        {
            if (!IsBackupEnabled)
            {
                LoggingService.LogInfo("[SessionManager] Session backup is disabled.");
                return;
            }

            // Serialize saves
            await _semaphoreSlim.WaitAsync();

            if (!IsBackupEnabled) return; // Check again after SemaphoreSlim released

            Stopwatch stopwatch = Stopwatch.StartNew();

            ITextEditor[] textEditors = _notepadsCore.GetAllTextEditors();

            if (textEditors == null || textEditors.Length == 0)
            {
                await ClearSessionDataAsync();
                actionAfterSaving?.Invoke();
                _semaphoreSlim.Release();
                return;
            }

            ITextEditor selectedTextEditor = _notepadsCore.GetSelectedTextEditor();
            FileSystemUtility.ClearFutureAccessList();

            NotepadsSessionData sessionData = new NotepadsSessionData();

            foreach (ITextEditor textEditor in textEditors)
            {
                if (textEditor.EditingFile != null)
                {
                    // Add the opened file to FutureAccessList so we can access it next launch
                    await FileSystemUtility.TryAddToFutureAccessList(ToToken(textEditor.Id), textEditor.EditingFile);
                }

                if (_sessionData.TryGetValue(textEditor.Id, out TextEditorSessionData textEditorData))
                {
                    // Get latest state meta data
                    textEditorData.StateMetaData = textEditor.GetTextEditorStateMetaData();
                }
                else // Text content has been changed or editor has not backed up yet
                {
                    textEditorData = new TextEditorSessionData
                    {
                        Id = textEditor.Id,
                    };

                    if (textEditor.EditingFile != null)
                    {
                        textEditorData.EditingFileFutureAccessToken = ToToken(textEditor.Id);
                        textEditorData.EditingFileName = textEditor.EditingFileName;
                        textEditorData.EditingFilePath = textEditor.EditingFilePath;
                    }

                    if (textEditor.IsModified)
                    {
                        if (textEditor.EditingFile != null)
                        {
                            // Persist the last save known to the app, which might not be up-to-date (if the file was modified outside the app)
                            var lastSavedBackupFile = await SessionUtility.CreateNewFileInBackupFolderAsync(ToToken(textEditor.Id) + "-LastSaved",
                                CreationCollisionOption.ReplaceExisting);

                            if (!await BackupTextAsync(textEditor.LastSavedSnapshot.Content,
                                textEditor.LastSavedSnapshot.Encoding,
                                textEditor.LastSavedSnapshot.LineEnding,
                                lastSavedBackupFile))
                            {
                                continue;
                            }

                            textEditorData.LastSavedBackupFilePath = lastSavedBackupFile.Path;
                        }

                        if (textEditor.EditingFile == null || !string.Equals(textEditor.LastSavedSnapshot.Content, textEditor.GetText()))
                        {
                            // Persist pending changes relative to the last save
                            var pendingBackupFile = await SessionUtility.CreateNewFileInBackupFolderAsync(ToToken(textEditor.Id) + "-Pending",
                                CreationCollisionOption.ReplaceExisting);

                            if (!await BackupTextAsync(textEditor.GetText(),
                                    textEditor.LastSavedSnapshot.Encoding,
                                    textEditor.LastSavedSnapshot.LineEnding,
                                    pendingBackupFile))
                            {
                                continue;
                            }

                            textEditorData.PendingBackupFilePath = pendingBackupFile.Path;
                        }
                    }

                    textEditorData.StateMetaData = textEditor.GetTextEditorStateMetaData();

                    // We will not create new backup files for this text editor unless it has changes
                    _sessionData.TryAdd(textEditor.Id, textEditorData);
                }

                sessionData.TextEditors.Add(textEditorData);

                if (textEditor == selectedTextEditor)
                {
                    sessionData.SelectedTextEditor = textEditor.Id;
                }
            }

            sessionData.TabScrollViewerHorizontalOffset =
                _notepadsCore.GetSetsViewScrollViewerHorizontalOffset();

            bool sessionDataSaved = false;

            try
            {
                string sessionJsonStr = JsonConvert.SerializeObject(sessionData, Formatting.Indented);

                if (_lastSessionJsonStr == null || !string.Equals(_lastSessionJsonStr, sessionJsonStr, StringComparison.OrdinalIgnoreCase))
                {
                    // write
                    await SessionUtility.SaveSerializedSessionMetaDataAsync(sessionJsonStr);
                    _lastSessionJsonStr = sessionJsonStr;
                    sessionDataSaved = true;

                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionManager] Failed to save session metadata: {ex.Message}");
                actionAfterSaving?.Invoke();
                _semaphoreSlim.Release();
                return; // Failed to save the session - do not proceed to delete backup files
            }

            if (sessionDataSaved)
            {
                await DeleteOrphanedBackupFilesAsync(sessionData);
            }

            stopwatch.Stop();

            if (sessionDataSaved)
            {
                LoggingService.LogInfo($"[SessionManager] Successfully saved the current session. Total time: {stopwatch.Elapsed.TotalMilliseconds} milliseconds.", consoleOnly: true);
            }

            actionAfterSaving?.Invoke();
            _semaphoreSlim.Release();
        }

        public void StartSessionBackup(bool startImmediately = false)
        {
            if (_timer == null)
            {
                Timer timer = new Timer(async (obj) => await SaveSessionAsync());

                if (Interlocked.CompareExchange(ref _timer, timer, null) == null)
                {
                    var delay = startImmediately ? TimeSpan.Zero : SaveInterval;
                    timer.Change(delay, SaveInterval);
                }
                else
                {
                    timer.Dispose();
                }
            }
        }

        public void StopSessionBackup()
        {
            Timer timer = _timer;

            try
            {
                timer?.Dispose();
            }
            catch
            {
                // Best effort
            }
            finally
            {
                Interlocked.CompareExchange(ref _timer, null, timer);
            }
        }

        public async Task ClearSessionDataAsync()
        {
            _lastSessionJsonStr = null;
            try
            {
                await SessionUtility.DeleteSerializedSessionMetaDataAsync();
                LoggingService.LogInfo($"[SessionManager] Successfully deleted session meta data.");
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionManager] Failed to delete session meta data: {ex.Message}");
            }
        }

        private void BindEditorContentStateChangeEvent(object sender, ITextEditor textEditor)
        {
            // All text or file related events
            textEditor.TextChanging += RemoveTextEditorSessionData;
            textEditor.ChangeReverted += RemoveTextEditorSessionData;
            textEditor.FileSaved += RemoveTextEditorSessionData;
            textEditor.FileReloaded += RemoveTextEditorSessionData;
        }

        private void UnbindEditorContentStateChangeEvent(object sender, ITextEditor textEditor)
        {
            // All text or file related events
            textEditor.TextChanging -= RemoveTextEditorSessionData;
            textEditor.ChangeReverted -= RemoveTextEditorSessionData;
            textEditor.FileSaved -= RemoveTextEditorSessionData;
            textEditor.FileReloaded -= RemoveTextEditorSessionData;
        }

        private async Task<ITextEditor> RecoverTextEditorAsync(TextEditorSessionData editorSessionData)
        {
            StorageFile editingFile = null;

            if (editorSessionData.EditingFileFutureAccessToken != null)
            {
                editingFile = await FileSystemUtility.GetFileFromFutureAccessList(editorSessionData.EditingFileFutureAccessToken);
            }

            string lastSavedFile = editorSessionData.LastSavedBackupFilePath;
            string pendingFile = editorSessionData.PendingBackupFilePath;

            ITextEditor textEditor;

            if (editingFile == null && lastSavedFile == null && pendingFile == null)
            {
                textEditor = null;
            }
            else if (editingFile != null && lastSavedFile == null && pendingFile == null) // File without pending changes
            {
                textEditor = await _notepadsCore.CreateTextEditor(editorSessionData.Id, editingFile, ignoreFileSizeLimit: true);
                textEditor.ResetEditorState(editorSessionData.StateMetaData);
            }
            else // File with pending changes
            {
                string lastSavedText = string.Empty;
                string pendingText = null;

                if (lastSavedFile != null)
                {
                    TextFile lastSavedTextFile = await FileSystemUtility.ReadFile(lastSavedFile, ignoreFileSizeLimit: true,
                    EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding));
                    lastSavedText = lastSavedTextFile.Content;
                }

                var textFile = new TextFile(lastSavedText,
                    EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(editorSessionData.StateMetaData.LastSavedLineEnding),
                    editorSessionData.StateMetaData.DateModifiedFileTime);

                textEditor = _notepadsCore.CreateTextEditor(
                    editorSessionData.Id,
                    textFile,
                    editingFile,
                    editorSessionData.StateMetaData.IsModified);

                if (pendingFile != null)
                {
                    TextFile pendingTextFile = await FileSystemUtility.ReadFile(pendingFile,
                        ignoreFileSizeLimit: true,
                        EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding));
                    pendingText = pendingTextFile.Content;
                }

                textEditor.ResetEditorState(editorSessionData.StateMetaData, pendingText);
            }

            return textEditor;
        }

        private async Task<bool> BackupTextAsync(string text, Encoding encoding, LineEnding lineEnding, StorageFile file)
        {
            try
            {
                await FileSystemUtility.WriteToFile(LineEndingUtility.ApplyLineEnding(text, lineEnding), encoding, file);
                return true;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[SessionManager] Failed to save backup file: {ex.Message}");
                return false;
            }
        }

        private async Task DeleteOrphanedBackupFilesAsync(NotepadsSessionData sessionData)
        {
            HashSet<string> backupPaths = sessionData.TextEditors
                .SelectMany(te => new[] { te.LastSavedBackupFilePath, te.PendingBackupFilePath })
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            foreach (StorageFile backupFile in await SessionUtility.GetAllBackupFilesAsync())
            {
                if (!backupPaths.Contains(backupFile.Path))
                {
                    try
                    {
                        await backupFile.DeleteAsync();
                    }
                    catch (Exception ex)
                    {
                        LoggingService.LogError($"[SessionManager] Failed to delete backup file: {ex.Message}");
                    }
                }
            }
        }

        private string ToToken(Guid textEditorId)
        {
            return textEditorId.ToString("N");
        }

        private void RemoveTextEditorSessionData(object sender, EventArgs e)
        {
            if (sender is ITextEditor textEditor)
            {
                _sessionData.TryRemove(textEditor.Id, out _);
            }
        }

        private class NotepadsSessionData
        {
            public NotepadsSessionData()
            {
                TextEditors = new List<TextEditorSessionData>();
            }

            public Guid SelectedTextEditor { get; set; }

            public double TabScrollViewerHorizontalOffset { get; set; }

            public List<TextEditorSessionData> TextEditors { get; }
        }

        private class TextEditorSessionData
        {
            public Guid Id { get; set; }

            public string LastSavedBackupFilePath { get; set; }

            public string PendingBackupFilePath { get; set; }

            public string EditingFileFutureAccessToken { get; set; }

            public string EditingFileName { get; set; }

            public string EditingFilePath { get; set; }

            public TextEditorStateMetaData StateMetaData { get; set; }
        }
    }
}
