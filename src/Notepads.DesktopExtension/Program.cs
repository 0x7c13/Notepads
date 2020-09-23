namespace Notepads.DesktopExtension
{
    using Microsoft.AppCenter;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.AppCenter.Crashes;
    using Notepads.Settings;
    using System;
    using System.Collections.Generic;
    using System.Diagnostics;
    using System.Globalization;
    using System.IO;
    using System.IO.MemoryMappedFiles;
    using System.IO.Pipes;
    using System.Linq;
    using System.Security.Principal;
    using System.Threading;
    using System.Threading.Tasks;
    using System.Windows.Forms;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.Foundation.Collections;
    using Windows.Storage;
    using Windows.System;
    using Windows.System.UserProfile;

    static class Program
    {
        private const string DesktopExtensionMutexName = "DesktopExtensionMutexName";
        private const string AdminExtensionMutexName = "AdminExtensionMutexName";

        private static readonly int sessionId = Process.GetCurrentProcess().SessionId;
        private static readonly string packageSID = ReadSettingsKey(SettingsKey.PackageSidStr) as string;

        private static AppServiceConnection connection = null;

        /// <summary>
        ///  The main entry point for the application.
        /// </summary>
        static void Main(string[] args)
        {
            AppDomain.CurrentDomain.UnhandledException += OnUnhandledException;
            Application.ThreadException += (sender, args) => OnUnhandledException(sender, new UnhandledExceptionEventArgs(args.Exception, true));
            TaskScheduler.UnobservedTaskException += OnUnobservedException;

            if (new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            {
                if (!IsFirstInstance(AdminExtensionMutexName)) return;
                InitializeExtensionService();
            }
            else
            {
                if (!IsFirstInstance(DesktopExtensionMutexName)) return;
                InitializeAppServiceConnection();
                if (args.Length > 2 && args[2] == "/admin") CreateElevetedExtension();
            }

            var services = new Type[] { typeof(Crashes), typeof(Analytics) };
            AppCenter.Start(SettingsKey.AppCenterSecret, services);

            Application.Run();
        }

        private static async void InitializeAppServiceConnection()
        {
            PrintDebugMessage("Successfully started Desktop Extension.");

            connection = new AppServiceConnection()
            {
                AppServiceName = "DesktopExtensionServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            connection.RequestReceived += Connection_RequestReceived;
            connection.ServiceClosed += Connection_ServiceClosed;

            AppServiceConnectionStatus status = await connection.OpenAsync();

            if (status != AppServiceConnectionStatus.Success)
            {
                Application.Exit();
            }

            var message = new ValueSet { { SettingsKey.InteropCommandLabel, SettingsKey.RegisterExtensionCommandStr } };
            await connection.SendMessageAsync(message);

            PrintDebugMessage("Successfully created App Service.");
        }

        private static void InitializeExtensionService()
        {
            PrintDebugMessage("Successfully started Adminstrator Extension.");
            PrintDebugMessage("Waiting on uwp app to send data.");

            SaveFileFromPipeData();
        }

        private static bool IsFirstInstance(string mutexName)
        {
            var instanceHandlerMutex = new Mutex(true, mutexName, out var isFirstInstance);

            if (!isFirstInstance)
            {
                PrintDebugMessage("Closing this instance as another instance is already running.", 5000);
            }

            return isFirstInstance;
        }

        private static void Connection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            // connection to the UWP lost, so we shut down the desktop process
            Application.Exit();
        }

        private static void Connection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            // Get a deferral because we use an awaitable API below to respond to the message
            // and we don't want this call to get canceled while we are waiting.
            var messageDeferral = args.GetDeferral();

            var message = args.Request.Message;
            if (!message.ContainsKey(SettingsKey.InteropCommandLabel) || !(message[SettingsKey.InteropCommandLabel] is string command)) return;
            switch (command)
            {
                case SettingsKey.CreateElevetedExtensionCommandStr:
                    CreateElevetedExtension();
                    break;
                case SettingsKey.ExitAppCommandStr:
                    Application.Exit();
                    break;
            }

            messageDeferral.Complete();
        }

        private static async void CreateElevetedExtension()
        {
            var message = new ValueSet { { SettingsKey.InteropCommandLabel, SettingsKey.CreateElevetedExtensionCommandStr } };

            try
            {
                ProcessStartInfo info = new ProcessStartInfo
                {
                    Verb = "runas",
                    UseShellExecute = true,
                    FileName = Application.ExecutablePath
                };

                var process = Process.Start(info);
                AppDomain.CurrentDomain.ProcessExit += (a, b) => process.Kill();
                message.Add(SettingsKey.InteropCommandAdminCreatedLabel, true);

                PrintDebugMessage("Adminstrator Extension has been launched.");
                Analytics.TrackEvent("OnAdminstratorPrivilageGranted", new Dictionary<string, string> { { "Accepted", true.ToString() } });
            }
            catch (Exception ex)
            {
                message.Add(SettingsKey.InteropCommandAdminCreatedLabel, false);

                PrintDebugMessage("User canceled launching of Adminstrator Extension.");
                Analytics.TrackEvent("OnAdminstratorPrivilageDenied",
                    new Dictionary<string, string> {
                        { "Denied", true.ToString() },
                        { "Message", ex.Message },
                        { "Exception", ex.ToString() },
                        { "InnerException", ex.InnerException?.ToString() },
                        { "InnerExceptionMessage", ex.InnerException?.Message }
                    });
            }
            finally
            {
                await connection.SendMessageAsync(message);
            }
        }

        private static async void SaveFileFromPipeData()
        {
            var result = "Failed";

            using var clientStream = new NamedPipeClientStream(".",
                $"Sessions\\{sessionId}\\AppContainerNamedObjects\\{packageSID}\\{Package.Current.Id.FamilyName}\\{SettingsKey.AdminPipeConnectionNameStr}",
                PipeDirection.InOut, PipeOptions.Asynchronous);

            // Wait for uwp app to send request to write to file.
            await clientStream.ConnectAsync();

            // Start another thread to accept more write requests.
            new Thread(new ThreadStart(SaveFileFromPipeData)).Start();

            var pipeReader = new StreamReader(clientStream);
            var pipeWriter = new StreamWriter(clientStream);

            var writeData = pipeReader.ReadLine().Split(new string[] { "?:" }, StringSplitOptions.None);
            var filePath = writeData[0];
            var memoryMapName = $"AppContainerNamedObjects\\{packageSID}\\{writeData[1]}";

            try
            {
                if (!int.TryParse(writeData[2], out int dataArrayLength)) throw new Exception("Failed to read piped data");

                // Open the memory-mapped file and read data from it.
                using (var reader = new BinaryReader(MemoryMappedFile.OpenExisting(memoryMapName).CreateViewStream()))
                {
                    var data = reader.ReadBytes(dataArrayLength);

                    await File.WriteAllBytesAsync(filePath, data);
                }

                result = "Success";

                PrintDebugMessage($"Successfully wrote to \"{filePath}\".");
            }
            catch (Exception ex)
            {
                PrintDebugMessage($"Failed to write to \"{filePath}\".");
                Analytics.TrackEvent("OnWriteToSystemFileRequested",
                    new Dictionary<string, string> {
                        { "Result", result },
                        { "Message", ex.Message },
                        { "Exception", ex.ToString() },
                        { "InnerException", ex.InnerException?.ToString() },
                        { "InnerExceptionMessage", ex.InnerException?.Message }
                    });
            }
            finally
            {
                pipeWriter.WriteLine(result);
                pipeWriter.Flush();

                PrintDebugMessage("Waiting on uwp app to send data.");
                if ("Success".Equals(result))
                {
                    Analytics.TrackEvent("OnWriteToSystemFileRequested", new Dictionary<string, string> { { "Result", result } });
                }
            }
        }

        public static object ReadSettingsKey(string key)
        {
            object obj = null;

            ApplicationDataContainer localSettings = ApplicationData.Current.LocalSettings;
            if (localSettings.Values.ContainsKey(key))
            {
                obj = localSettings.Values[key];
            }

            return obj;
        }

        private static void PrintDebugMessage(string message, int waitAfterPrintingTime = 0)
        {
#if DEBUG
            Console.WriteLine(message);
            Debug.WriteLine(message);
            Thread.Sleep(waitAfterPrintingTime);
#endif
        }

        private static void OnUnhandledException(object sender, UnhandledExceptionEventArgs e)
        {
            var exception = e.ExceptionObject as Exception;

            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", exception?.Message },
                { "Exception", exception?.ToString() },
                { "Culture", (GlobalizationPreferences.Languages.Count > 0 ? new CultureInfo(GlobalizationPreferences.Languages.First()) : null).EnglishName },
                { "AvailableMemory", ((float)MemoryManager.AppMemoryUsageLimit / 1024 / 1024).ToString("F0") },
                { "FirstUseTimeUTC", Process.GetCurrentProcess().StartTime.ToUniversalTime().ToString("MM/dd/yyyy HH:mm:ss") },
                { "OSArchitecture", Package.Current.Id.Architecture.ToString() },
                { "OSVersion", Environment.OSVersion.Version.ToString() },
                { "IsElevated", new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator).ToString() }
            };

            var attachment = ErrorAttachmentLog.AttachmentWithText(
                $"Exception: {exception}, " +
                $"Message: {exception?.Message}, " +
                $"InnerException: {exception?.InnerException}, " +
                $"InnerExceptionMessage: {exception?.InnerException?.Message}",
                "UnhandledException");

            Analytics.TrackEvent("OnUnhandledException", diagnosticInfo);
            Crashes.TrackError(exception, diagnosticInfo, attachment);

            // handle it manually.
            if (e.IsTerminating) Application.Restart();
        }

        private static void OnUnobservedException(object sender, UnobservedTaskExceptionEventArgs e)
        {
            var diagnosticInfo = new Dictionary<string, string>()
            {
                { "Message", e.Exception?.Message },
                { "Exception", e.Exception?.ToString() },
                { "InnerException", e.Exception?.InnerException?.ToString() },
                { "InnerExceptionMessage", e.Exception?.InnerException?.Message }
            };

            var attachment = ErrorAttachmentLog.AttachmentWithText(
                $"Exception: {e.Exception}, " +
                $"Message: {e.Exception?.Message}, " +
                $"InnerException: {e.Exception?.InnerException}, " +
                $"InnerExceptionMessage: {e.Exception?.InnerException?.Message}, " +
                $"IsDesktopExtension: {true}",
                "UnobservedException");

            Analytics.TrackEvent("OnUnobservedException", diagnosticInfo);
            Crashes.TrackError(e.Exception, diagnosticInfo, attachment);

            // suppress and handle it manually.
            e.SetObserved();
        }
    }
}
