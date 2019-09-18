namespace Notepads.Core
{
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
    }
}