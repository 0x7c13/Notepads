
namespace Notepads.Core
{
    using System.Threading.Tasks;

    internal interface ISessionManager
    {
        bool IsBackupEnabled { get; set; }

        Task<int> LoadLastSessionAsync();

        Task SaveSessionAsync();

        void StartSessionBackup();

        void StopSessionBackup();

        void ClearSessionData();
    }
}