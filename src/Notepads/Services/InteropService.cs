namespace Notepads.Services
{
    using Notepads.AdminService;
    using Notepads.Extensions;
    using Notepads.Settings;
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.ApplicationModel.Resources;
    using Windows.Foundation.Collections;
    using Windows.UI.Xaml;

    public static class InteropService
    {
        public static EventHandler<bool> HideSettingsPane;
        public static EventHandler<bool> UpdateRecentList;
        public static bool EnableSettingsLogging = true;

        public static AppServiceConnection InteropServiceConnection = null;
        public static AdminServiceClient AdminServiceClient = new AdminServiceClient();
        public static readonly Exception AdminstratorAccessException = new Exception("Failed to save due to no Adminstration access");
        private static readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        private static readonly string _commandLabel = "Command";
        private static readonly string _failedLabel = "Failed";
        private static readonly string _settingsKeyLabel = "Settings";
        private static readonly string _valueLabel = "Value";
        private static readonly string _adminCreatedLabel = "AdminCreated";

        private static IReadOnlyDictionary<string, Action<object>> SettingsManager = new Dictionary<string, Action<object>>
        {
            {SettingsKey.AlwaysOpenNewWindowBool, SettingsSyncService.AlwaysOpenNewWindow},
            {SettingsKey.AppAccentColorHexStr, SettingsSyncService.AppAccentColor},
            {SettingsKey.AppBackgroundTintOpacityDouble, SettingsSyncService.AppBackgroundPanelTintOpacity},
            {SettingsKey.CustomAccentColorHexStr, SettingsSyncService.CustomAccentColor},
            {SettingsKey.EditorCustomMadeSearchUrlStr, SettingsSyncService.EditorCustomMadeSearchUrl},
            {SettingsKey.EditorDefaultDecodingCodePageInt, SettingsSyncService.EditorDefaultDecoding},
            {SettingsKey.EditorDefaultEncodingCodePageInt, SettingsSyncService.EditorDefaultEncoding},
            {SettingsKey.EditorDefaultLineEndingStr, SettingsSyncService.EditorDefaultLineEnding},
            {SettingsKey.EditorDefaultLineHighlighterViewStateBool, SettingsSyncService.EditorDisplayLineHighlighter},
            {SettingsKey.EditorDefaultDisplayLineNumbersBool, SettingsSyncService.EditorDisplayLineNumbers},
            {SettingsKey.EditorDefaultSearchEngineStr, SettingsSyncService.EditorDefaultSearchEngine},
            {SettingsKey.EditorDefaultTabIndentsInt, SettingsSyncService.EditorDefaultTabIndents},
            {SettingsKey.EditorDefaultTextWrappingStr, SettingsSyncService.EditorDefaultTextWrapping},
            {SettingsKey.EditorFontFamilyStr, SettingsSyncService.EditorFontFamily},
            {SettingsKey.EditorFontSizeInt, SettingsSyncService.EditorFontSize},
            {SettingsKey.EditorFontStyleStr, SettingsSyncService.EditorFontStyle},
            {SettingsKey.EditorFontWeightUshort, SettingsSyncService.EditorFontWeight},
            {SettingsKey.EditorHighlightMisspelledWordsBool, SettingsSyncService.IsHighlightMisspelledWordsEnabled},
            {SettingsKey.EditorShowStatusBarBool, SettingsSyncService.ShowStatusBar},
            {SettingsKey.UseWindowsAccentColorBool, SettingsSyncService.UseWindowsAccentColor},
            {SettingsKey.RequestedThemeStr, SettingsSyncService.ThemeMode}
        };

        public static async void Initialize()
        {
            InteropServiceConnection = new AppServiceConnection()
            {
                AppServiceName = "InteropServiceConnection",
                PackageFamilyName = Package.Current.Id.FamilyName
            };

            InteropServiceConnection.RequestReceived += InteropServiceConnection_RequestReceived;
            InteropServiceConnection.ServiceClosed += InteropServiceConnection_ServiceClosed;

            AppServiceConnectionStatus status = await InteropServiceConnection.OpenAsync();
            if (status != AppServiceConnectionStatus.Success) Application.Current.Exit();
        }

        private static async void InteropServiceConnection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse(typeof(CommandArgs), (string)message[_commandLabel], out var result)) return;
            var command = (CommandArgs)result;
            switch(command)
            {
                case CommandArgs.SyncSettings:
                    EnableSettingsLogging = false;
                    HideSettingsPane?.Invoke(null, true);
                    var settingsKey = message[_settingsKeyLabel] as string;
                    var value = message[_valueLabel] as object;
                    SettingsManager[settingsKey](value);
                    EnableSettingsLogging = true;
                    break;
                case CommandArgs.SyncRecentList:
                    UpdateRecentList?.Invoke(null, false);
                    break;
                case CommandArgs.CreateElevetedExtension:
                    await DispatcherExtensions.CallOnUIThreadAsync(SettingsSyncService.Dispatcher, () =>
                    {
                        if (message.ContainsKey(_adminCreatedLabel) && (bool)message[_adminCreatedLabel])
                        {
                            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreated"), 1500);
                        }
                        else
                        {
                            NotificationCenter.Instance.PostNotification(_resourceLoader.GetString("TextEditor_NotificationMsg_AdminExtensionCreationFailed"), 1500);
                        }
                    });
                    break;
            }
        }

        private static void InteropServiceConnection_ServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
        {
            return;
        }

        public static async void SyncSettings(string settingsKey, object value)
        {
            if (InteropServiceConnection == null) return;

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.SyncSettings.ToString());
            message.Add(_settingsKeyLabel, settingsKey);
            try
            {
                message.Add(_valueLabel, value);
            }
            catch (Exception)
            {
                message.Add(_valueLabel, value.ToString());
            }
            await InteropServiceConnection.SendMessageAsync(message);
        }

        public static async void SyncRecentList()
        {
            if (InteropServiceConnection == null) return;

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.SyncRecentList.ToString());
            await InteropServiceConnection.SendMessageAsync(message);
        }

        public static async Task SaveFileAsAdmin(string filePath, byte[] data)
        {
            if (InteropServiceConnection == null) return;

            bool isAdminExtensionAvailable = true;
            bool failedFromAdminExtension = false;

            try
            {
                if (await AdminServiceClient.IsLiveAsync())
                {
                    ApplicationSettingsStore.Write(SettingsKey.AdminAuthenticationTokenStr, Guid.NewGuid().ToString());
                    failedFromAdminExtension = !await AdminServiceClient.SaveFileAsync(filePath, data);
                }
            }
            catch
            {
                isAdminExtensionAvailable = false;
            }
            finally
            {
                ApplicationSettingsStore.Remove(SettingsKey.AdminAuthenticationTokenStr);
            }

            if (!isAdminExtensionAvailable)
            {
                throw AdminstratorAccessException;
            }

            if (failedFromAdminExtension)
            {
                throw new UnauthorizedAccessException();
            }
        }

        public static async Task CreateElevetedExtension()
        {
            if (InteropServiceConnection == null) return;

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.CreateElevetedExtension.ToString());
            var response = await InteropServiceConnection.SendMessageAsync(message);
            message = response.Message;

            if (message.ContainsKey(_failedLabel) && (bool)message[_failedLabel])
            {
                await FullTrustProcessLauncher.LaunchFullTrustProcessForCurrentAppAsync("LaunchAdmin");
            }
        }
    }
}
