namespace Notepads.Core
{
    using Notepads.Controls.TextEditor;
    using Notepads.Core.SessionDataModels;
    using System;
    using System.Threading.Tasks;

    internal interface ISessionManager
    {
        bool IsBackupEnabled { get; set; }

        Task<int> LoadLastSessionAsync();

        Task SaveSessionAsync(Action actionAfterSaving = null);

        void StartSessionBackup(bool startImmediately = false);

        void StopSessionBackup();

        Task ClearSessionDataAsync();

        Task<ITextEditor> RecoverTextEditorAsync(TextEditorSessionDataV1 editorSessionData);
    }
}