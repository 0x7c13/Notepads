
namespace Notepads.Core
{
    using Newtonsoft.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.Storage;

    internal class SessionManager : ISessionManager
    {
        private const string SessionDataKey = "NotepadsSessionDataV1";
        private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);

        private readonly INotepadsCore _notepadsCore;
        private Timer _timer;
        private readonly SemaphoreSlim _semaphoreSlim;
        private readonly EncodingConverter _encodingConverter;
        private bool _loaded;

        /// <summary>
        /// Do not call this constructor directly. Use <see cref="SessionUtility.GetSessionManager(INotepadsCore)" instead./>
        /// </summary>
        public SessionManager(INotepadsCore notepadsCore)
        {
            _notepadsCore = notepadsCore;
            _semaphoreSlim = new SemaphoreSlim(1, 1);
            _encodingConverter = new EncodingConverter();
            _loaded = false;
        }

        public bool IsBackupEnabled { get; set; }

        public async Task<int> LoadLastSessionAsync()
        {
            if (_loaded)
            {
                return 0; // Already loaded
            }

            if (!ApplicationData.Current.LocalSettings.Values.TryGetValue(SessionDataKey, out object data))
            {
                return 0; // No session data found
            }

            NotepadsSessionDataV1 sessionData;

            try
            {
                sessionData = JsonConvert.DeserializeObject<NotepadsSessionDataV1>((string)data, _encodingConverter);
            }
            catch
            {
                return 0; // Failed to load the session data
            }

            int recoveredCount = 0;
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
                    recoveredCount++;

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

            return recoveredCount;
        }

        public async Task SaveSessionAsync()
        {
            if (!IsBackupEnabled)
            {
                return;
            }

            // Serialize saves
            await _semaphoreSlim.WaitAsync();

            NotepadsSessionDataV1 sessionData = new NotepadsSessionDataV1();
            HashSet<string> backupPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            TextEditor[] textEditors = _notepadsCore.GetAllTextEditors();
            TextEditor selectedTextEditor = _notepadsCore.GetSelectedTextEditor();

            FileSystemUtility.ClearFutureAccessList();

            foreach (TextEditor textEditor in textEditors)
            {
                TextEditorSessionData textEditorData = new TextEditorSessionData { Id = textEditor.Id };

                if (textEditor.EditingFile != null)
                {
                    // Add the opened file to FutureAccessList so we can access it next launch
                    FileSystemUtility.TryAddToFutureAccessList(ToToken(textEditor.Id), textEditor.EditingFile);

                    // Persist the last save known to the app, which might not be up-to-date (if the file was modified outside the app)
                    BackupMetadata lastSaved = await SaveLastSavedChangesAsync(textEditor);

                    if (lastSaved == null)
                    {
                        continue;
                    }

                    textEditorData.LastSaved = lastSaved;
                    backupPaths.Add(lastSaved.BackupFilePath);
                }

                if (textEditor.IsModified)
                {
                    // Persist pending changes relative to the last save
                    BackupMetadata pending = await SavePendingChangesAsync(textEditor);

                    if (pending == null)
                    {
                        continue;
                    }

                    textEditorData.Pending = pending;
                    backupPaths.Add(pending.BackupFilePath);
                }

                if (textEditorData.LastSaved != null || textEditorData.Pending != null)
                {
                    sessionData.TextEditors.Add(textEditorData);

                    if (textEditor == selectedTextEditor)
                    {
                        sessionData.SelectedTextEditor = textEditor.Id;
                    }
                }
            }

            try
            {
                string sessionJson = JsonConvert.SerializeObject(sessionData, _encodingConverter);
                ApplicationData.Current.LocalSettings.Values[SessionDataKey] = sessionJson;
                LoggingService.LogInfo("Successfully saved the current session.");
            }
            catch
            {
                return; // Failed to save the session - do not proceed to delete backup files
            }

            await DeleteOrphanedBackupFilesAsync(backupPaths);

            _semaphoreSlim.Release();
        }

        public void StartSessionBackup()
        {
            if (_timer == null)
            {
                Timer timer = new Timer(async (obj) => await SaveSessionAsync());

                if (Interlocked.CompareExchange(ref _timer, timer, null) == null)
                {
                    timer.Change(SaveInterval, SaveInterval);
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
            ApplicationData.Current.LocalSettings.Values.Remove(SessionDataKey);
        }

        private async Task<TextEditor> RecoverTextEditorAsync(TextEditorSessionData textEditorData)
        {
            StorageFile sourceFile = await FileSystemUtility.GetFileFromFutureAccessList(ToToken(textEditorData.Id));
            BackupMetadata lastSaved = textEditorData.LastSaved;
            BackupMetadata pending = textEditorData.Pending;
            TextEditor textEditor;

            if (sourceFile == null)
            {
                textEditor = _notepadsCore.OpenNewTextEditor(
                    textEditorData.Id,
                    string.Empty,
                    null,
                    -1,
                    EditorSettingsService.EditorDefaultEncoding,
                    EditorSettingsService.EditorDefaultLineEnding,
                    false);

                await ApplyChangesAsync(textEditor, pending ?? lastSaved);
            }
            else
            {
                TextFile lastSavedContent = await FileSystemUtility.ReadFile(lastSaved.BackupFilePath);

                textEditor = _notepadsCore.OpenNewTextEditor(
                    textEditorData.Id,
                    lastSavedContent.Content,
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
            if (backupMetadata == null)
            {
                return;
            }

            TextFile textFile = await FileSystemUtility.ReadFile(backupMetadata.BackupFilePath);
            textEditor.Init(textFile, textEditor.EditingFile, resetOriginalSnapshot: false, isModified: true);
            textEditor.TryChangeEncoding(backupMetadata.Encoding);
            textEditor.TryChangeLineEnding(backupMetadata.LineEnding);
        }

        private async Task<BackupMetadata> SaveLastSavedChangesAsync(TextEditor textEditor)
        {
            TextFile originalSnapshot = textEditor.OriginalSnapshot;
            StorageFile backupFile;

            try
            {
                backupFile = await SessionUtility.CreateNewBackupFileAsync(textEditor.Id.ToString("N") + "-LastSaved");
                await FileSystemUtility.WriteToFile(LineEndingUtility.ApplyLineEnding(originalSnapshot.Content, originalSnapshot.LineEnding), originalSnapshot.Encoding, backupFile);
            }
            catch
            {
                return null;
            }

            return new BackupMetadata
            {
                BackupFilePath = backupFile.Path,
                Encoding = originalSnapshot.Encoding,
                LineEnding = originalSnapshot.LineEnding,
                DateModified = originalSnapshot.DateModifiedFileTime
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

        private async Task DeleteOrphanedBackupFilesAsync(HashSet<string> backupPaths)
        {
            foreach (StorageFile backupFile in await SessionUtility.GetAllBackupFilesAsync())
            {
                if (!backupPaths.Contains(backupFile.Path))
                {
                    try
                    {
                        await backupFile.DeleteAsync();
                    }
                    catch
                    {
                        // Best effort
                    }
                }
            }
        }

        private string ToToken(Guid textEditorId)
        {
            return textEditorId.ToString("N");
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
                return EncodingUtility.GetEncodingByName((string)reader.Value);
            }

            public override void WriteJson(JsonWriter writer, Encoding value, JsonSerializer serializer)
            {
                writer.WriteValue(EncodingUtility.GetEncodingBodyName(value));
            }
        }
    }
}
