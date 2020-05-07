using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Principal;
using System.ServiceModel;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.ApplicationModel.AppService;
using Windows.Foundation.Collections;
using Windows.Storage;

public enum CommandArgs
{
    SyncSettings,
    SyncRecentList,
    RegisterExtension,
    CreateElevetedExtension,
    ReplaceFile
}

namespace Notepads.DesktopExtension
{
    static class Program
    {
        private static AppServiceConnection connection = null;
        private static readonly string _commandLabel = "Command";
        private static readonly string _newFileLabel = "From";
        private static readonly string _oldFileLabel = "To";
        private static readonly string _failureLabel = "Failed";

        private static ServiceHost selfHost = null;

        /// <summary>
        ///  The main entry point for the application.
        /// </summary>
        static void Main(string[] args)
        {
            if (new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            {
                InitializeExtensionService();
            }
            else
            {
                InitializeAppServiceConnection();
            }

            Application.Run();
        }

        private static async void InitializeAppServiceConnection()
        {
            connection = new AppServiceConnection()
            {
                AppServiceName = "InteropServiceConnection",
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
            selfHost = new ServiceHost(typeof(ExtensionService));
            selfHost.Open();
        }

        private static void Connection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            // connection to the UWP lost, so we shut down the desktop process
            Application.Exit();
        }

        private static async void Connection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            // Get a deferral because we use an awaitable API below to respond to the message
            // and we don't want this call to get canceled while we are waiting.
            var messageDeferral = args.GetDeferral();

            try
            {
                var message = args.Request.Message;
                if (!message.ContainsKey(_commandLabel) || !Enum.TryParse<CommandArgs>((string)message[_commandLabel], out var command)) return;
                switch (command)
                {
                    case CommandArgs.ReplaceFile:
                        var status = false;
                        status = ReplaceFile((string)message[_newFileLabel], (string)message[_oldFileLabel]);
                        message.Clear();
                        message.Add(_failureLabel, true);
                        await args.Request.SendResponseAsync(message);
                        break;
                    case CommandArgs.CreateElevetedExtension:
                        CreateElevetedExtension();
                        break;
                }
            }
            catch (Exception)
            {
                MessageBox.Show("error");
            }

            messageDeferral.Complete();
        }

        public static bool ReplaceFile(string newPath, string oldPath)
        {
            try
            {
                if (File.Exists(oldPath)) File.Delete(oldPath);
                File.Move(newPath, oldPath);
                if (File.Exists(newPath)) File.Delete(newPath);
            }
            catch (Exception)
            {
                return false;
            }
            return true;
        }

        private static void CreateElevetedExtension()
        {
            string result = Assembly.GetExecutingAssembly().Location;
            int index = result.LastIndexOf("\\");
            string rootPath = $"{result.Substring(0, index)}\\..\\";
            string aliasPath = rootPath + @"\Notepads.DesktopExtension\Notepads.DesktopExtension.exe";

            ProcessStartInfo info = new ProcessStartInfo();
            info.Verb = "runas";
            info.UseShellExecute = true;
            info.FileName = aliasPath;
            Process.Start(info);
        }
    }
}
