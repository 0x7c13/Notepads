namespace Notepads.Services
{
    using Microsoft.AppCenter;
    using Notepads.Controls.Dialog;
    using Notepads.Core;
    using Notepads.Settings;
    using Notepads.Utilities;
    using System;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.IO.Pipes;
    using System.Security.Principal;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.Resources;
    using Windows.Foundation.Metadata;
    using Windows.Security.Authentication.Web;
    using Windows.Storage;
    using Windows.Storage.AccessCache;

    public enum ElevatedOperationType
    {
        Save,
        Rename
    }

    public class AdminstratorAccessException : Exception
    {
        public AdminstratorAccessException():base("Failed to save due to no Adminstration access") { }

        public AdminstratorAccessException(string message) : base(message) { }
        public AdminstratorAccessException(string message, Exception innerException) : base(message, innerException) { }
    }

    public static class DesktopExtensionService
    {
        public static readonly bool ShouldUseDesktopExtension = 
            ApiInformation.IsApiContractPresent("Windows.ApplicationModel.FullTrustAppContract", 1, 0) &&
            !new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator);

        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private static Mutex ExtensionProcessLifetimeObject = null;
        private static Mutex ElevatedProcessLifetimeObject = null;

        private static EventWaitHandle _extensionUnblockEvent = null;
        private static EventWaitHandle _elevatedWriteEvent = null;
        private static EventWaitHandle _elevatedRenameEvent = null;

        /// <summary>
        /// Initializes desktop processes.
        /// </summary>
        public static async void Initialize()
        {
            if (!ShouldUseDesktopExtension) return;

            if (ExtensionProcessLifetimeObject == null)
            {
                ExtensionProcessLifetimeObject = new Mutex(false, CoreKey.ExtensionProcessLifetimeObjNameStr);
            }

            if (ElevatedProcessLifetimeObject == null)
            {
                ElevatedProcessLifetimeObject = new Mutex(false, CoreKey.ElevatedProcessLifetimeObjNameStr);
            }

            if (_extensionUnblockEvent == null)
            {
                _extensionUnblockEvent = new EventWaitHandle(false, EventResetMode.ManualReset, CoreKey.ExtensionUnblockEventNameStr);
            }

            if (_elevatedWriteEvent == null)
            {
                _elevatedWriteEvent = new EventWaitHandle(false, EventResetMode.ManualReset, CoreKey.ElevatedWriteEventNameStr);
            }

            if (_elevatedRenameEvent == null)
            {
                _elevatedRenameEvent = new EventWaitHandle(false, EventResetMode.ManualReset, CoreKey.ElevatedRenameEventNameStr);
            }

            ApplicationSettingsStore.Write(CoreKey.PackageSidStr, WebAuthenticationBroker.GetCurrentApplicationCallbackUri().Host.ToUpper());
            ApplicationSettingsStore.Write(CoreKey.AppCenterInstallIdStr, (await AppCenter.GetInstallIdAsync())?.ToString());

            await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync();
        }

        /// <summary>
        /// Releases resources and closes desktop processes.
        /// </summary>
        public static void Dispose()
        {
            _extensionUnblockEvent?.Dispose();
            _elevatedWriteEvent?.Dispose();
            _elevatedRenameEvent?.Dispose();
            ExtensionProcessLifetimeObject?.Dispose();
            ElevatedProcessLifetimeObject?.Dispose();
        }

        /// <summary>
        /// Unblock UWP edited file with desktop component.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        public static async void UnblockFile(string filePath)
        {
            if (!ShouldUseDesktopExtension) return;

            using (var pipeStream = new NamedPipeServerStream(
                $"Local\\{CoreKey.ExtensionUnblockPipeConnectionNameStr}",
                PipeDirection.Out,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                _extensionUnblockEvent.Set();
                // Wait for 100 ms for desktop extension to accept request.
                if (!pipeStream.WaitForConnectionAsync().Wait(100))
                {
                    // If the connection fails, desktop extension is not launched.
                    // In that case, launch desktop extension.
                    _extensionUnblockEvent.Reset();
                    await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync();
                }
                else
                {
                    var pipeWriter = new StreamWriter(pipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));

                    await pipeWriter.WriteAsync(filePath);
                    await pipeWriter.FlushAsync();
                }
            }
        }

        /// <summary>
        /// Launch desktop extension with elevated privilages.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        public static async Task LaunchElevetedProcess()
        {
            if (!ShouldUseDesktopExtension) return;

            // Pass the group id that describes the correct parameter for prompting elevated process launch
            await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync("Elevate");
        }

        /// <summary>
        /// Notify user whether launching of eleveted process succeeded.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="isLaunched">Is eleveted process launched.</param>
        public static void OnElevetedProcessLaunchRequested(bool isLaunched)
        {
            if (isLaunched)
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_ElevatedProcessLaunched"), 1500);
            }
            else
            {
                NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_ElevatedProcessLaunchFailed"), 1500);
            }
        }

        /// <summary>
        /// Write data to system file.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="filePath">Absolute path of the file to write.</param>
        /// <param name="data">Data to write.</param>
        public static async Task SaveFileAsAdmin(string filePath, byte[] data)
        {
            if (!ShouldUseDesktopExtension) return;

            using (var pipeStream = new NamedPipeServerStream(
                $"Local\\{CoreKey.ElevatedWritePipeConnectionNameStr}",
                PipeDirection.InOut,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                _elevatedWriteEvent.Set();
                // Wait for 100 ms for desktop extension to accept request.
                if (!pipeStream.WaitForConnectionAsync().Wait(100))
                {
                    // If the connection fails, desktop extension is not launched with elevated privilages.
                    // In that case, prompt user to launch desktop extension with elevated privilages.
                    _elevatedWriteEvent.Reset();
                    pipeStream?.Dispose();
                    throw new AdminstratorAccessException();
                }

                var pipeReader = new StreamReader(pipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));
                var pipeWriter = new StreamWriter(pipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));

                var mapName = filePath.Replace(Path.DirectorySeparatorChar, '-');

                // Create the memory-mapped file and write original file data to be written.
                using (var mmf = MemoryMappedFile.CreateOrOpen(mapName, data.Length > 0 ? data.Length : 1, MemoryMappedFileAccess.ReadWrite))
                {
                    using (var writer = new BinaryWriter(mmf.CreateViewStream()))
                    {
                        writer.Write(data);
                        writer.Flush();
                    }

                    await pipeWriter.WriteAsync($"{filePath}|{mapName}|{data.Length}");
                    await pipeWriter.FlushAsync();

                    // Wait for desktop extension to send response.
                    if (!"Success".Equals(await pipeReader.ReadLineAsync()))
                    {
                        // Promt user to "Save As" if extension failed to save data.
                        mmf?.Dispose();
                        pipeStream?.Dispose();
                        throw new UnauthorizedAccessException();
                    }
                }
            }
        }

        /// <summary>
        /// Rename system file.
        /// </summary>
        /// <remarks>
        /// Only available for legacy Windows 10 desktop.
        /// </remarks>
        /// <param name="file">File to rename</param>
        /// <param name="newName">New name</param>
        public static async Task<StorageFile> RenameFileAsAdmin(StorageFile file, string newName)
        {
            if (!ShouldUseDesktopExtension) return null;

            using (var pipeStream = new NamedPipeServerStream(
                $"Local\\{CoreKey.ElevatedRenamePipeConnectionNameStr}",
                PipeDirection.InOut,
                NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous))
            {
                _elevatedRenameEvent.Set();
                // Wait for 100 ms for desktop extension to accept request.
                if (!pipeStream.WaitForConnectionAsync().Wait(100))
                {
                    // If the connection fails, desktop extension is not launched with elevated privilages.
                    // In that case, prompt user to launch desktop extension with elevated privilages.
                    _elevatedRenameEvent.Reset();

                    var launchElevatedProcessDialog = new LaunchElevatedProcessDialog(
                        ElevatedOperationType.Rename,
                        file.Path,
                        async () => { await LaunchElevetedProcess(); },
                        null);

                    await DialogManager.OpenDialogAsync(launchElevatedProcessDialog, awaitPreviousDialog: false);
                    file = null;
                }
                else
                {
                    var pipeReader = new StreamReader(pipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));
                    var pipeWriter = new StreamWriter(pipeStream, new System.Text.UnicodeEncoding(!BitConverter.IsLittleEndian, false));

                    var token = StorageApplicationPermissions.FutureAccessList.Add(file);
                    await pipeWriter.WriteAsync($"{token}|{newName}");
                    await pipeWriter.FlushAsync();

                    // Wait for desktop extension to send response.
                    token = await pipeReader.ReadLineAsync();
                    file = await StorageApplicationPermissions.FutureAccessList.GetFileAsync(token);
                    StorageApplicationPermissions.FutureAccessList.Remove(token);
                }
            }

            return file;
        }
    }
}
