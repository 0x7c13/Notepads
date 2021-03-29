﻿namespace Notepads.Core
{
    using System;
    using System.Collections.Concurrent;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.IO;
    using System.Linq;
    using System.Text;
    using System.Threading;
    using System.Threading.Tasks;
    using Microsoft.AppCenter.Analytics;
    using System.Text.Json;
    using Notepads.Controls.TextEditor;
    using Notepads.Core.SessionDataModels;
    using Notepads.Models;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.Storage.AccessCache;

    internal class SessionManager : ISessionManager, IDisposable
    {
        private static readonly TimeSpan SaveInterval = TimeSpan.FromSeconds(7);
        private readonly INotepadsCore _notepadsCore;
        private readonly SemaphoreSlim _semaphoreSlim;
        private readonly ConcurrentDictionary<Guid, TextEditorSessionDataV1> _sessionDataCache;
        private string _lastSessionJsonStr;
        private bool _loaded;
        private Timer _timer;

        private readonly string _backupFolderName;
        private readonly string _sessionMetaDataFileName;

        /// <summary>
        /// Do not call this constructor directly. Use <see cref="SessionUtility.GetSessionManager(INotepadsCore)" instead./>
        /// </summary>
        public SessionManager(INotepadsCore notepadsCore, string backupFolderName, string sessionMetaDataFileName)
        {
            _notepadsCore = notepadsCore;
            _backupFolderName = backupFolderName;
            _sessionMetaDataFileName = sessionMetaDataFileName;
            _semaphoreSlim = new SemaphoreSlim(1, 1);
            _sessionDataCache = new ConcurrentDictionary<Guid, TextEditorSessionDataV1>();
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

            IList<ITextEditor> recoveredEditor = new List<ITextEditor>();
            var sessionData = await SessionUtility.GetSessionMetaDataAsync(_sessionMetaDataFileName);
            var backupFiles = (await SessionUtility.GetAllBackupFilesAsync(_backupFolderName)).ToList();
            var orphanedEditorCount = 0;

            if (sessionData == null && backupFiles.Count > 0)
            {
                recoveredEditor = await RecoverOrphanedTextEditors(backupFiles);
                orphanedEditorCount = recoveredEditor.Count;
                _notepadsCore.OpenTextEditors(recoveredEditor.ToArray());
                await ClearSessionDataAsync();
                return orphanedEditorCount; // No session meta-data found, only recover from orphaned backup files
            }

            foreach (var textEditorData in sessionData.TextEditors)
            {
                Tuple<ITextEditor, IList<StorageFile>> recoveredData;

                try
                {
                    recoveredData = await RecoverTextEditorAsync(textEditorData, backupFiles);
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[{nameof(SessionManager)}] Failed to recover TextEditor: {ex.Message}");
                    Analytics.TrackEvent("SessionManager_FailedToRecoverTextEditor", new Dictionary<string, string>() { { "Exception", ex.Message } });
                    continue;
                }

                if (recoveredData != null)
                {
                    if (recoveredData.Item1 != null)
                    {
                        recoveredEditor.Add(recoveredData.Item1);
                        _sessionDataCache.TryAdd(recoveredData.Item1.Id, textEditorData);
                    }
                    
                    if (recoveredData.Item2?.Count > 0)
                    {
                        recoveredData.Item2.All(file => backupFiles.Remove(file));
                    }
                }
            }

            if (backupFiles?.Count > 0)
            {
                var orphanedEditors = await RecoverOrphanedTextEditors(backupFiles);
                orphanedEditorCount = orphanedEditors.Count;
                recoveredEditor.Concat(orphanedEditors);
            }

            _notepadsCore.OpenTextEditors(recoveredEditor.ToArray(), sessionData.SelectedTextEditor);
            _notepadsCore.SetTabScrollViewerHorizontalOffset(sessionData.TabScrollViewerHorizontalOffset);

            _loaded = true;

            LoggingService.LogInfo($"[{nameof(SessionManager)}] {_sessionDataCache.Count} tab(s) restored from last session.");

            return _sessionDataCache.Count + orphanedEditorCount;
        }

        public async Task SaveSessionAsync(Action actionAfterSaving = null)
        {
            if (!IsBackupEnabled)
            {
                LoggingService.LogInfo($"[{nameof(SessionManager)}] Session backup is disabled.");
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

            NotepadsSessionDataV1 sessionData = new NotepadsSessionDataV1();

            foreach (ITextEditor textEditor in textEditors)
            {
                try
                {
                    var textEditorSessionData = await GetTextEditorSessionData(textEditor);

                    if (textEditorSessionData == null) continue;

                    sessionData.TextEditors.Add(textEditorSessionData);

                    if (textEditor == selectedTextEditor)
                    {
                        sessionData.SelectedTextEditor = textEditor.Id;
                    }
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[{nameof(SessionManager)}] Failed to build TextEditor session data: {ex}");
                    Analytics.TrackEvent("SessionManager_FailedToBuildTextEditorSessionData", new Dictionary<string, string>() { { "Exception", ex.Message } });
                }
            }

            sessionData.TabScrollViewerHorizontalOffset =
                _notepadsCore.GetTabScrollViewerHorizontalOffset();

            bool sessionDataSaved = false;

            try
            {
                string sessionJsonStr = JsonSerializer.Serialize(sessionData, new JsonSerializerOptions { WriteIndented = true });

                if (_lastSessionJsonStr == null || !string.Equals(_lastSessionJsonStr, sessionJsonStr, StringComparison.OrdinalIgnoreCase))
                {
                    // write
                    await SessionUtility.SaveSerializedSessionMetaDataAsync(sessionJsonStr, _sessionMetaDataFileName);
                    _lastSessionJsonStr = sessionJsonStr;
                    sessionDataSaved = true;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(SessionManager)}] Failed to save session metadata: {ex.Message}");
                Analytics.TrackEvent("SessionManager_FailedToSaveSessionMetaData", new Dictionary<string, string>() { { "Exception", ex.Message } });
                actionAfterSaving?.Invoke();
                _semaphoreSlim.Release();
                return; // Failed to save the session - do not proceed to delete backup files
            }

            if (sessionDataSaved)
            {
                try
                {
                    await DeleteOrphanedBackupFilesAsync(sessionData);
                    DeleteOrphanedTokensInFutureAccessList(sessionData);
                }
                catch (Exception ex)
                {
                    Analytics.TrackEvent("SessionManager_FailedToDeleteOrphanedBackupFiles",
                        new Dictionary<string, string>() { { "Exception", ex.Message } });
                }
            }

            stopwatch.Stop();

            if (sessionDataSaved)
            {
                LoggingService.LogInfo($"[{nameof(SessionManager)}] Successfully saved the current session. Total time: {stopwatch.Elapsed.TotalMilliseconds} milliseconds.", consoleOnly: true);
            }

            actionAfterSaving?.Invoke();
            _semaphoreSlim.Release();
        }

        private async Task<TextEditorSessionDataV1> GetTextEditorSessionData(ITextEditor textEditor)
        {
            if (_sessionDataCache.TryGetValue(textEditor.Id, out TextEditorSessionDataV1 textEditorSessionData))
            {
                // We will not create new backup files for this text editor unless it has content changes
                // But we should update latest TextEditor state meta data
                textEditorSessionData.StateMetaData = textEditor.GetTextEditorStateMetaData();
            }
            else // Text content has been changed or editor has not backed up yet
            {
                textEditorSessionData = await BuildTextEditorSessionData(textEditor);

                if (textEditorSessionData == null)
                {
                    return null;
                }

                _sessionDataCache.TryAdd(textEditor.Id, textEditorSessionData);
            }

            return textEditorSessionData;
        }

        private async Task<TextEditorSessionDataV1> BuildTextEditorSessionData(ITextEditor textEditor)
        {
            TextEditorSessionDataV1 textEditorData = new TextEditorSessionDataV1
            {
                Id = textEditor.Id,
            };

            if (textEditor.EditingFile != null)
            {
                // Add the opened file to FutureAccessList so we can access it next launch
                var futureAccessToken = ToToken(textEditor.Id);
                await FutureAccessListUtility.TryAddOrReplaceTokenInFutureAccessList(futureAccessToken, textEditor.EditingFile);
                textEditorData.EditingFileFutureAccessToken = futureAccessToken;
                textEditorData.EditingFileName = textEditor.EditingFileName;
                textEditorData.EditingFilePath = textEditor.EditingFilePath;
            }

            if (textEditor.IsModified)
            {
                if (textEditor.EditingFile != null)
                {
                    // Persist the last save known to the app, which might not be up-to-date (if the file was modified outside the app)
                    var lastSavedBackupFile = await SessionUtility.CreateNewFileInBackupFolderAsync(
                        ToToken(textEditor.Id) + "-LastSaved",
                        CreationCollisionOption.ReplaceExisting,
                        _backupFolderName);

                    if (!await BackupTextAsync(textEditor.LastSavedSnapshot.Content,
                        textEditor.LastSavedSnapshot.Encoding,
                        textEditor.LastSavedSnapshot.LineEnding,
                        lastSavedBackupFile))
                    {
                        return null; // Error: Failed to write backup text to file
                    }

                    textEditorData.LastSavedBackupFilePath = lastSavedBackupFile.Path;
                }

                if (textEditor.EditingFile == null ||
                    !string.Equals(textEditor.LastSavedSnapshot.Content, textEditor.GetText()))
                {
                    // Persist pending changes relative to the last save
                    var pendingBackupFile = await SessionUtility.CreateNewFileInBackupFolderAsync(
                        ToToken(textEditor.Id) + "-Pending",
                        CreationCollisionOption.ReplaceExisting,
                        _backupFolderName);

                    if (!await BackupTextAsync(textEditor.GetText(),
                        textEditor.LastSavedSnapshot.Encoding,
                        textEditor.LastSavedSnapshot.LineEnding,
                        pendingBackupFile))
                    {
                        return null; // Error: Failed to write backup text to file
                    }

                    textEditorData.PendingBackupFilePath = pendingBackupFile.Path;
                }
            }

            textEditorData.StateMetaData = textEditor.GetTextEditorStateMetaData();

            return textEditorData;
        }

        public void StartSessionBackup(bool startImmediately = false)
        {
            if (_timer == null)
            {
                LoggingService.LogInfo($"[{nameof(SessionManager)}] Session backup process started (StartImmediately = {startImmediately}).");

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
                await SessionUtility.DeleteSerializedSessionMetaDataAsync(_sessionMetaDataFileName);
                LoggingService.LogInfo($"[{nameof(SessionManager)}] Successfully deleted session meta data.");
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(SessionManager)}] Failed to delete session meta data: {ex.Message}");
                Analytics.TrackEvent("SessionManager_FailedToDeleteSessionMetaData", new Dictionary<string, string>() { { "Exception", ex.Message } });
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

        private async Task<Tuple<ITextEditor, IList<StorageFile>>> RecoverTextEditorAsync(
            TextEditorSessionDataV1 editorSessionData,
            IList<StorageFile> backupFiles)
        {
            StorageFile editingFile = null;
            IList<StorageFile> recoveredFiles = new List<StorageFile>();

            if (editorSessionData.EditingFileFutureAccessToken != null)
            {
                editingFile = await FutureAccessListUtility.GetFileFromFutureAccessList(editorSessionData.EditingFileFutureAccessToken);
            }

            string lastSavedFilePath = editorSessionData.LastSavedBackupFilePath;
            string pendingFilePath = editorSessionData.PendingBackupFilePath;

            ITextEditor textEditor;

            if (editingFile == null && lastSavedFilePath == null && pendingFilePath == null)
            {
                textEditor = null;
            }
            else if (editingFile != null && lastSavedFilePath == null && pendingFilePath == null) // File without pending changes
            {
                var encoding = EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding);
                textEditor = await _notepadsCore.CreateTextEditor(editorSessionData.Id, editingFile, encoding: encoding, ignoreFileSizeLimit: true);
                textEditor.ResetEditorState(editorSessionData.StateMetaData);
            }
            else // File with pending changes
            {
                string lastSavedText = string.Empty;
                string pendingText = null;

                if (lastSavedFilePath != null)
                {
                    var lastSavedFile = backupFiles.First(file => lastSavedFilePath.Equals(file.Path, StringComparison.OrdinalIgnoreCase));
                    if (lastSavedFile == null)
                    {
                        lastSavedText = (await FileSystemUtility.ReadFile(lastSavedFilePath, ignoreFileSizeLimit: true,
                            EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding))).Content;
                    }
                    else
                    {
                        recoveredFiles.Add(lastSavedFile);
                        lastSavedText = (await FileSystemUtility.ReadFile(lastSavedFile, ignoreFileSizeLimit: true,
                            EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding))).Content;
                    }
                }

                var textFile = new TextFile(lastSavedText,
                    EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding),
                    LineEndingUtility.GetLineEndingByName(editorSessionData.StateMetaData.LastSavedLineEnding),
                    editorSessionData.StateMetaData.DateModifiedFileTime);

                textEditor = _notepadsCore.CreateTextEditor(
                    editorSessionData.Id,
                    textFile,
                    editingFile,
                    editorSessionData.StateMetaData.FileNamePlaceholder,
                    editorSessionData.StateMetaData.IsModified);

                if (pendingFilePath != null)
                {
                    var pendingFile = backupFiles.First(file => pendingFilePath.Equals(file.Path, StringComparison.OrdinalIgnoreCase));
                    if (pendingFile == null)
                    {
                        pendingText = (await FileSystemUtility.ReadFile(pendingFilePath, ignoreFileSizeLimit: true,
                            EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding))).Content;
                    }
                    else
                    {
                        recoveredFiles.Add(pendingFile);
                        pendingText = (await FileSystemUtility.ReadFile(pendingFile, ignoreFileSizeLimit: true,
                            EncodingUtility.GetEncodingByName(editorSessionData.StateMetaData.LastSavedEncoding))).Content;
                    }
                }

                textEditor.ResetEditorState(editorSessionData.StateMetaData, pendingText);
            }

            return Tuple.Create(textEditor, recoveredFiles);
        }

        // Recover editos from backup files if they have failed to store metadata
        private async Task<IList<ITextEditor>> RecoverOrphanedTextEditors(IList<StorageFile> backupFiles)
        {
            IList<ITextEditor> recoveredEditors = new List<ITextEditor>();
            while (backupFiles.Count > 0)
            {
                var backupFile = backupFiles[0];
                Guid editorId = Guid.NewGuid();
                TextFile lastSavedTextFile = null;
                TextFile pendingTextFile = null;
                if (backupFile.Name.EndsWith("-LastSaved"))
                {
                    editorId = Guid.Parse(backupFile.Name.Replace("-LastSaved", ""));
                    lastSavedTextFile = await FileSystemUtility.ReadFile(backupFile, ignoreFileSizeLimit: true);
                    var pendingFile = backupFiles.First(file => file.Name.Equals(ToToken(editorId) + "-Pending"));
                    pendingTextFile = await FileSystemUtility.ReadFile(pendingFile, ignoreFileSizeLimit: true);

                    backupFiles.Remove(backupFile);
                    backupFiles.Remove(pendingFile);
                }
                else if (backupFile.Name.EndsWith("-Pending"))
                {
                    editorId = Guid.Parse(backupFile.Name.Replace("-Pending", ""));
                    var lastSavedFile = backupFiles.First(file => file.Name.Equals(ToToken(editorId) + "-LastSaved"));
                    lastSavedTextFile = await FileSystemUtility.ReadFile(lastSavedFile, ignoreFileSizeLimit: true);
                    pendingTextFile = await FileSystemUtility.ReadFile(backupFile, ignoreFileSizeLimit: true);

                    backupFiles.Remove(backupFile);
                    backupFiles.Remove(lastSavedFile);
                }
                else
                {
                    backupFiles.Remove(backupFile);
                    continue;
                }

                var failedPendingFilePath = Path.Combine(SessionUtility.GetBackupFolderPath(_backupFolderName),
                    ToToken(editorId) + "-Pending.~TMP");
                if (pendingTextFile == null)
                {
                    if (File.Exists(failedPendingFilePath))
                    {
                        pendingTextFile = await FileSystemUtility.ReadLocalFile(failedPendingFilePath);
                    }
                    else
                    {
                        // If there are no pending changes no need to recover
                        continue;
                    }
                }

                ITextEditor textEditor = null;
                var defaultName = ResourceLoader.GetForCurrentView().GetString("TextEditor_DefaultNewFileName");
                var editingFile = await FutureAccessListUtility.GetFileFromFutureAccessList(ToToken(editorId));
                var textFile = await FileSystemUtility.ReadFile(editingFile, ignoreFileSizeLimit: true);
                if (lastSavedTextFile == null)
                {
                    textEditor = _notepadsCore.CreateTextEditor(editorId,
                        textFile ?? pendingTextFile,
                        editingFile,
                        editingFile?.Name ?? defaultName,
                        true);
                }
                else
                {
                    textEditor = _notepadsCore.CreateTextEditor(editorId,
                        lastSavedTextFile,
                        editingFile,
                        editingFile?.Name ?? defaultName,
                        true);
                }

                if (textEditor == null) continue;

                textEditor.ResetEditorState(textEditor.GetTextEditorStateMetaData(), pendingTextFile.Content);

                if (File.Exists(failedPendingFilePath))
                {
                    var failedPendingText = await File.ReadAllTextAsync(failedPendingFilePath);
                    textEditor.ResetEditorState(textEditor.GetTextEditorStateMetaData(), failedPendingText);
                }

                recoveredEditors.Add(textEditor);
            }

            return recoveredEditors;
        }

        private static async Task<bool> BackupTextAsync(string text, Encoding encoding, LineEnding lineEnding, StorageFile file)
        {
            try
            {
                await FileSystemUtility.SafeWriteToFile(LineEndingUtility.ApplyLineEnding(text, lineEnding), encoding, file);
                return true;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(SessionManager)}] Failed to save backup file: {ex.Message}");
                return false;
            }
        }

        // Cleanup orphaned/dangling backup files
        private async Task DeleteOrphanedBackupFilesAsync(NotepadsSessionDataV1 sessionData)
        {
            HashSet<string> backupPaths = sessionData.TextEditors
                .SelectMany(editor => new[] { editor.LastSavedBackupFilePath, editor.PendingBackupFilePath })
                .Where(path => path != null)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            foreach (StorageFile backupFile in await SessionUtility.GetAllBackupFilesAsync(_backupFolderName))
            {
                if (!backupPaths.Contains(backupFile.Path))
                {
                    try
                    {
                        await backupFile.DeleteAsync();
                    }
                    catch (Exception ex)
                    {
                        LoggingService.LogError($"[{nameof(SessionManager)}] Failed to delete orphaned backup file: {ex.Message}");
                    }
                }
            }

            foreach (var filePth in Directory.GetFiles(ApplicationData.Current.LocalFolder.Path, "*.~TMP", SearchOption.AllDirectories))
            {
                File.Delete(filePth);
            }
        }

        // Cleanup orphaned/dangling entries in FutureAccessList
        private void DeleteOrphanedTokensInFutureAccessList(NotepadsSessionDataV1 sessionData)
        {
            HashSet<string> tokens = sessionData.TextEditors
                .SelectMany(editor => new[] { editor.EditingFileFutureAccessToken })
                .Where(token => token != null)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            var tokensToBeDeleted = new List<string>();

            foreach (var entry in StorageApplicationPermissions.FutureAccessList.Entries)
            {
                if (!tokens.Contains(entry.Token))
                {
                    tokensToBeDeleted.Add(entry.Token);
                }
            }

            foreach (var tokenToBeDel in tokensToBeDeleted)
            {
                try
                {
                    StorageApplicationPermissions.FutureAccessList.Remove(tokenToBeDel);
                }
                catch (Exception ex)
                {
                    LoggingService.LogError($"[{nameof(SessionManager)}] Failed to delete orphaned token in FutureAccessList: {ex.Message}");
                    Analytics.TrackEvent("SessionManager_FailedToDeleteOrphanedTokenInFutureAccessList", new Dictionary<string, string>() { { "Exception", ex.Message } });
                }
            }
        }

        private static string ToToken(Guid textEditorId)
        {
            return textEditorId.ToString("N");
        }

        private void RemoveTextEditorSessionData(object sender, EventArgs e)
        {
            if (sender is ITextEditor textEditor)
            {
                _sessionDataCache.TryRemove(textEditor.Id, out _);
            }
        }

        public void Dispose()
        {
            if (_notepadsCore != null)
            {
                _notepadsCore.TextEditorLoaded -= BindEditorContentStateChangeEvent;
                _notepadsCore.TextEditorUnloaded -= UnbindEditorContentStateChangeEvent;
            }

            _semaphoreSlim?.Dispose();
            _timer?.Dispose();
        }
    }
}