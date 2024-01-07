// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

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

        Task<int> RecoverBackupFilesAsync();

        Task OpenSessionBackupFolderAsync();
    }
}