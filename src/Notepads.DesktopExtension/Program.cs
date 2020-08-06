namespace Notepads.DesktopExtension
{
    using Notepads.DesktopExtension.Services;
    using System;
    using System.Diagnostics;
    using System.Reflection;
    using System.Security.Principal;
    using System.ServiceModel;
    using System.Threading;
    using System.Windows.Forms;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.Foundation.Collections;

    public enum CommandArgs
    {
        RegisterExtension,
        CreateElevetedExtension,
        ExitApp
    }

    static class Program
    {
        private const string DesktopExtensionMutexName = "DesktopExtensionMutexName";
        private const string AdminExtensionMutexName = "AdminExtensionMutexName";

        private static readonly string _commandLabel = "Command";
        private static readonly string _adminCreatedLabel = "AdminCreated";

        private static AppServiceConnection connection = null;
        private static ServiceHost selfHost = null;

        /// <summary>
        ///  The main entry point for the application.
        /// </summary>
        static void Main(string[] args)
        {
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

            Application.Run();
        }

        private static async void InitializeAppServiceConnection()
        {
            connection = new AppServiceConnection()
            {
                AppServiceName = "DesktopExtensionServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            connection.RequestReceived += Connection_RequestReceived;
            connection.ServiceClosed += Connection_ServiceClosed;

            AppServiceConnectionStatus status = await connection.OpenAsync();

            if(status != AppServiceConnectionStatus.Success)
            {
                Application.Exit();
            }

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.RegisterExtension.ToString());
            await connection.SendMessageAsync(message);
        }

        private static void InitializeExtensionService()
        {
            selfHost = new ServiceHost(typeof(AdminService));
            selfHost.Open();
        }

        private static bool IsFirstInstance(string mutexName)
        {
            var instanceHandlerMutex = new Mutex(true, mutexName, out var isFirstInstance);

            if (isFirstInstance)
            {
                instanceHandlerMutex.ReleaseMutex();
            }
            else
            {
                instanceHandlerMutex.Close();
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
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse<CommandArgs>((string)message[_commandLabel], out var command)) return;
            switch (command)
            {
                case CommandArgs.CreateElevetedExtension:
                    CreateElevetedExtension();
                    break;
                case CommandArgs.ExitApp:
                    Application.Exit();
                    break;
            }

            messageDeferral.Complete();
        }

        private static async void CreateElevetedExtension()
        {
            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.CreateElevetedExtension.ToString());
            try
            {
                string result = Assembly.GetExecutingAssembly().Location;
                int index = result.LastIndexOf("\\");
                string rootPath = $"{result.Substring(0, index)}\\..\\";
                string aliasPath = rootPath + @"\Notepads.DesktopExtension\Notepads32.exe";

                ProcessStartInfo info = new ProcessStartInfo();
                info.Verb = "runas";
                info.UseShellExecute = true;
                info.FileName = aliasPath;
                var process = Process.Start(info);
                AppDomain.CurrentDomain.ProcessExit += (a, b) => process.Kill();
                message.Add(_adminCreatedLabel, true);
            }
            catch
            {
                message.Add(_adminCreatedLabel, false);
            }
            finally
            {
                await connection.SendMessageAsync(message);
            }
        }
    }
}
