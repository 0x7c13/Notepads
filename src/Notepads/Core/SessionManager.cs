
namespace Notepads.Core
{
    using Newtonsoft.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.Linq;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.Foundation.Collections;
    using Windows.Storage;

    internal class SessionManager : ISessionManager
    {
        private const string SessionDataKey = "NotepadsSessionDataV1";
        private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);

        private readonly INotepadsCore _notepadsCore;
        private readonly SemaphoreSlim _semaphoreSlim;
        private readonly EncodingConverter _encodingConverter;
        private readonly ConcurrentDictionary<Guid, TextEditorSessionData> _sessionData;
        private bool _loaded;
        private Timer _timer;

        /// <summary>
        /// Do not call this constructor directly. Use <see cref="SessionUtility.GetSessionManager(INotepadsCore)" instead./>
        /// </summary>
        public SessionManager(INotepadsCore notepadsCore)
        {
            _notepadsCore = notepadsCore;
            _semaphoreSlim = new SemaphoreSlim(1, 1);
            _encodingConverter = new EncodingConverter();
            _sessionData = new ConcurrentDictionary<Guid, TextEditorSessionData>();
            _loaded = false;

            foreach (var editor in _notepadsCore.GetAllTextEditors())
            {
                BindEditorStateChangeEvent(this, editor);
            }

            _notepadsCore.TextEditorLoaded += BindEditorStateChangeEvent;
            _notepadsCore.TextEditorUnloaded += UnbindEditorStateChangeEvent;
        }

        public bool IsBackupEnabled { get; set; }

        public async Task<int> LoadLastSessionAsync()
        {
            if (_loaded)
            {
                return 0; // Already loaded
            }

            if (!(ApplicationSettingsStore.Read(SessionDataKey) is string data))
            {
                return 0; // No session data found
            }

            NotepadsSessionDataV1 sessionData;

            try
            {
                sessionData = JsonConvert.DeserializeObject<NotepadsSessionDataV1>(data, _encodingConverter);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to load session metadata: {ex.Message}");
                return 0;
            }

            TextEditor selectedTextEditor = null;

            foreach (TextEditorSessionData textEditorData in sessionData.TextEditors)
            {
                TextEditor textEditor;

                try
                {
                    textEditor = await RecoverTextEditorAsync(textEditorData);
                }
                catch
                {
                    continue;
                }

                if (textEditor != null)
                {
                    _sessionData.TryAdd(textEditor.Id, textEditorData);

                    if (textEditor.Id == sessionData.SelectedTextEditor)
                    {
                        selectedTextEditor = textEditor;
                    }
                }
            }

            if (selectedTextEditor != null)
            {
                _notepadsCore.SwitchTo(selectedTextEditor);
            }

            _loaded = true;

            return _sessionData.Count;
        }

        public async Task SaveSessionAsync()
        {
            if (!IsBackupEnabled)
            {
                LoggingService.LogInfo("Session backup is disabled.");
                return;
            }

            // Serialize saves
            await _semaphoreSlim.WaitAsync();

            Stopwatch stopwatch = Stopwatch.StartNew();

            TextEditor[] textEditors = _notepadsCore.GetAllTextEditors();
            TextEditor selectedTextEditor = _notepadsCore.GetSelectedTextEditor();
            FileSystemUtility.ClearFutureAccessList();

            NotepadsSessionDataV1 sessionData = new NotepadsSessionDataV1();

            foreach (TextEditor textEditor in textEditors)
            {
                if (textEditor.EditingFile != null)
                {
                    // Add the opened file to FutureAccessList so we can access it next launch
                    await FileSystemUtility.TryAddToFutureAccessList(ToToken(textEditor.Id), textEditor.EditingFile);
                }

                if (!_sessionData.TryGetValue(textEditor.Id, out TextEditorSessionData textEditorData))
                {
                    textEditorData = new TextEditorSessionData { Id = textEditor.Id };

                    if (textEditor.IsModified)
                    {
                        if (textEditor.EditingFile != null)
                        {
                            // Persist the last save known to the app, which might not be up-to-date (if the file was modified outside the app)
                            BackupMetadata lastSaved = await SaveLastSavedChangesAsync(textEditor);

                            if (lastSaved == null)
                            {
                                continue;
                            }

                            textEditorData.LastSaved = lastSaved;
                        }

                        // Persist pending changes relative to the last save
                        BackupMetadata pending = await SavePendingChangesAsync(textEditor);

                        if (pending == null)
                        {
                            continue;
                        }

                        textEditorData.Pending = pending;
                    }

                    // We will not create new backup files for this text editor unless it has changes
                    _sessionData.TryAdd(textEditor.Id, textEditorData);
                }

                sessionData.TextEditors.Add(textEditorData);

                if (textEditor == selectedTextEditor)
                {
                    sessionData.SelectedTextEditor = textEditor.Id;
                }
            }

            bool sessionDataSaved = false;

            try
            {
                string sessionJsonStr = JsonConvert.SerializeObject(sessionData, _encodingConverter);

                if (!(ApplicationSettingsStore.Read(SessionDataKey) is string currentValue) || !string.Equals(currentValue, sessionJsonStr, StringComparison.OrdinalIgnoreCase))
                {
                    ApplicationSettingsStore.Write(SessionDataKey, sessionJsonStr);
                    sessionDataSaved = true;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to save session metadata: {ex.Message}");
                return; // Failed to save the session - do not proceed to delete backup files
            }

            if (sessionDataSaved)
            {
                await DeleteOrphanedBackupFilesAsync(sessionData);
            }

            stopwatch.Stop();

            if (sessionDataSaved)
            {
                LoggingService.LogInfo($"Successfully saved the current session. Total time: {stopwatch.Elapsed.TotalMilliseconds} milliseconds.", consoleOnly: true);
            }

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

        public void ClearSessionData()
        {
            ApplicationSettingsStore.Remove(SessionDataKey);
        }

        private void BindEditorStateChangeEvent(object sender, TextEditor textEditor)
        {
            textEditor.TextChanging += RemoveTextEditorSessionData;
            textEditor.EncodingChanged += RemoveTextEditorSessionData;
            textEditor.LineEndingChanged += RemoveTextEditorSessionData;
            textEditor.ChangeReverted += RemoveTextEditorSessionData;
            textEditor.EditorModificationStateChanged += RemoveTextEditorSessionData;
        }

        private void UnbindEditorStateChangeEvent(object sender, TextEditor textEditor)
        {
            textEditor.TextChanging -= RemoveTextEditorSessionData;
            textEditor.EncodingChanged -= RemoveTextEditorSessionData;
            textEditor.LineEndingChanged -= RemoveTextEditorSessionData;
            textEditor.ChangeReverted -= RemoveTextEditorSessionData;
            textEditor.EditorModificationStateChanged -= RemoveTextEditorSessionData;
        }

        private async Task<TextEditor> RecoverTextEditorAsync(TextEditorSessionData textEditorData)
        {
            StorageFile sourceFile = await FileSystemUtility.GetFileFromFutureAccessList(ToToken(textEditorData.Id));
            BackupMetadata lastSaved = textEditorData.LastSaved;
            BackupMetadata pending = textEditorData.Pending;
            TextEditor textEditor;

            if (sourceFile == null) // Untitled.txt or file not found
            {
                if (lastSaved != null || pending != null)
                {
                    textEditor = _notepadsCore.OpenNewTextEditor(textEditorData.Id);
                    await ApplyChangesAsync(textEditor, pending ?? lastSaved);
                }
                else
                {
                    textEditor = null;
                }
            }
            else if (lastSaved == null && pending == null) // File without pending changes
            {
                textEditor = await _notepadsCore.OpenNewTextEditor(sourceFile, ignoreFileSizeLimit: true, textEditorData.Id);
            }
            else // File with pending changes
            {
                TextFile textFile = await FileSystemUtility.ReadFile(lastSaved.BackupFilePath, ignoreFileSizeLimit: true);

                textEditor = _notepadsCore.OpenNewTextEditor(
                    textEditorData.Id,
                    textFile.Content,
                    sourceFile,
                    lastSaved.DateModified,
                    lastSaved.Encoding,
                    lastSaved.LineEnding,
                    false);

                await ApplyChangesAsync(textEditor, pending);
            }

            return textEditor;
        }

        private async Task ApplyChangesAsync(TextEditor textEditor, BackupMetadata backupMetadata)
        {
            TextFile textFile = await FileSystemUtility.ReadFile(backupMetadata.BackupFilePath, ignoreFileSizeLimit: true);
            textEditor.Init(textFile, textEditor.EditingFile, resetLastSavedSnapshot: false, isModified: true);
            textEditor.TryChangeEncoding(backupMetadata.Encoding);
            textEditor.TryChangeLineEnding(backupMetadata.LineEnding);
        }

        private async Task<BackupMetadata> SaveLastSavedChangesAsync(TextEditor textEditor)
        {
            TextFile snapshot = textEditor.LastSavedSnapshot;
            StorageFile backupFile;

            try
            {
                backupFile = await SessionUtility.CreateNewBackupFileAsync(textEditor.Id.ToString("N") + "-LastSaved");
                await FileSystemUtility.WriteToFile(LineEndingUtility.ApplyLineEnding(snapshot.Content, snapshot.LineEnding), snapshot.Encoding, backupFile);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to save backup file: {ex.Message}");
                return null;
            }

            return new BackupMetadata
            {
                BackupFilePath = backupFile.Path,
                Encoding = snapshot.Encoding,
                LineEnding = snapshot.LineEnding,
                DateModified = snapshot.DateModifiedFileTime
            };
        }

        private async Task<BackupMetadata> SavePendingChangesAsync(TextEditor textEditor)
        {
            StorageFile backupFile;
            TextFile textFile;

            try
            {
                backupFile = await SessionUtility.CreateNewBackupFileAsync(textEditor.Id.ToString("N") + "-Pending");
                textFile = await textEditor.SaveContentToFile(backupFile);
            }
            catch
            {
                return null;
            }

            return new BackupMetadata
            {
                BackupFilePath = backupFile.Path,
                Encoding = textFile.Encoding,
                LineEnding = textFile.LineEnding,
                DateModified = textFile.DateModifiedFileTime
            };
        }

        private async Task DeleteOrphanedBackupFilesAsync(NotepadsSessionDataV1 sessionData)
        {
            HashSet<string> backupPaths = sessionData.TextEditors
                .SelectMany(te => new[] { te.LastSaved?.BackupFilePath, te.Pending?.BackupFilePath })
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
                        LoggingService.LogError($"Failed to delete backup file: {ex.Message}");
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
            if (sender is TextEditor textEditor)
            {
                _sessionData.TryRemove(textEditor.Id, out _);
            }
        }

        private class NotepadsSessionDataV1
        {
            public NotepadsSessionDataV1()
            {
                TextEditors = new List<TextEditorSessionData>();
            }

            public Guid SelectedTextEditor { get; set; }

            public List<TextEditorSessionData> TextEditors { get; }
        }

        private class TextEditorSessionData
        {
            public Guid Id { get; set; }

            public BackupMetadata LastSaved { get; set; }

            public BackupMetadata Pending { get; set; }
        }

        private class BackupMetadata
        {
            public string BackupFilePath { get; set; }

            public Encoding Encoding { get; set; }

            public LineEnding LineEnding { get; set; }

            public long DateModified { get; set; }
        }

        private class EncodingConverter : JsonConverter<Encoding>
        {
            public override Encoding ReadJson(JsonReader reader, Type objectType, Encoding existingValue, bool hasExistingValue, JsonSerializer serializer)
            {
                return EncodingUtility.GetEncodingByName((string)reader.Value, fallbackEncoding: new UTF8Encoding(false));
            }

            public override void WriteJson(JsonWriter writer, Encoding value, JsonSerializer serializer)
            {
                writer.WriteValue(EncodingUtility.GetEncodingName(value));
            }
        }
    }
}
