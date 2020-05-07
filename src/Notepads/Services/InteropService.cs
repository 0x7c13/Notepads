namespace Notepads.Services
{
    using Notepads.ExtensionService;
    using Notepads.Settings;
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
    using Windows.ApplicationModel.AppService;
    using Windows.Foundation.Collections;
    using Windows.UI.Xaml;

    public static class InteropService
    {
        public static EventHandler<bool> HideSettingsPane;
        public static EventHandler<bool> UpdateRecentList;
        public static bool EnableSettingsLogging = true;

        public static AppServiceConnection InteropServiceConnection = null;
        private static ExtensionServiceClient adminExtensionServiceClient = new ExtensionServiceClient();

        private static readonly string _commandLabel = "Command";
        private static readonly string _appIdLabel = "Instance";
        private static readonly string _settingsKeyLabel = "Settings";
        private static readonly string _valueLabel = "Value";
        private static readonly string _newFileLabel = "From";
        private static readonly string _oldFileLabel = "To";
        private static readonly string _failureLabel = "Failed";

        private static IReadOnlyDictionary<string,Settings> SettingsManager = new Dictionary<string, Settings>
        {
            {SettingsKey.AlwaysOpenNewWindowBool, SettingsDelegate.AlwaysOpenNewWindow},
            {SettingsKey.AppAccentColorHexStr, SettingsDelegate.AppAccentColor},
            {SettingsKey.AppBackgroundTintOpacityDouble, SettingsDelegate.AppBackgroundPanelTintOpacity},
            {SettingsKey.CustomAccentColorHexStr, SettingsDelegate.CustomAccentColor},
            {SettingsKey.EditorCustomMadeSearchUrlStr, SettingsDelegate.EditorCustomMadeSearchUrl},
            {SettingsKey.EditorDefaultDecodingCodePageInt, SettingsDelegate.EditorDefaultDecoding},
            {SettingsKey.EditorDefaultEncodingCodePageInt, SettingsDelegate.EditorDefaultEncoding},
            {SettingsKey.EditorDefaultLineEndingStr, SettingsDelegate.EditorDefaultLineEnding},
            {SettingsKey.EditorDefaultLineHighlighterViewStateBool, SettingsDelegate.IsLineHighlighterEnabled},
            {SettingsKey.EditorDefaultSearchEngineStr, SettingsDelegate.EditorDefaultSearchEngine},
            {SettingsKey.EditorDefaultTabIndentsInt, SettingsDelegate.EditorDefaultTabIndents},
            {SettingsKey.EditorDefaultTextWrappingStr, SettingsDelegate.EditorDefaultTextWrapping},
            {SettingsKey.EditorFontFamilyStr, SettingsDelegate.EditorFontFamily},
            {SettingsKey.EditorFontSizeInt, SettingsDelegate.EditorFontSize},
            {SettingsKey.EditorHighlightMisspelledWordsBool, SettingsDelegate.IsHighlightMisspelledWordsEnabled},
            {SettingsKey.EditorShowStatusBarBool, SettingsDelegate.ShowStatusBar},
            {SettingsKey.UseWindowsAccentColorBool, SettingsDelegate.UseWindowsAccentColor},
            {SettingsKey.RequestedThemeStr, SettingsDelegate.ThemeMode}
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

        private static void InteropServiceConnection_RequestReceived(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
        {
            var message = args.Request.Message;
            if (!message.ContainsKey(_commandLabel) || !Enum.TryParse(typeof(CommandArgs), (string)message[_commandLabel], out var result)) return;
            var command = (CommandArgs)result;
            switch(command)
            {
                case CommandArgs.SyncSettings:
                    if (message.ContainsKey(_appIdLabel) && message[_appIdLabel] is Guid appID && appID != App.Id)
                    {
                        EnableSettingsLogging = false;
                        HideSettingsPane?.Invoke(null, true);
                        var settingsKey = message[_settingsKeyLabel] as string;
                        var value = message[_valueLabel] as object;
                        SettingsManager[settingsKey](value);
                        EnableSettingsLogging = true;
                    }
                    break;
                case CommandArgs.SyncRecentList:
                    UpdateRecentList?.Invoke(null, false);
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
            message.Add(_appIdLabel, App.Id);
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

        public static async Task ReplaceFile(string newFilePath, string oldFilePath, bool isElevationRequired)
        {
            if (InteropServiceConnection == null) return;

            bool failedFromDesktopExtension = false;
            bool failedFromAdminExtension = false;
            if (isElevationRequired)
            {
                try
                {
                    if (!await adminExtensionServiceClient.ReplaceFileAsync(newFilePath, oldFilePath))
                    {
                        failedFromAdminExtension = true;
                    }
                }
                catch
                {
                    failedFromAdminExtension = true;
                }
            }
            else
            {
                var message = new ValueSet();
                message.Add(_commandLabel, CommandArgs.ReplaceFile.ToString());
                message.Add(_newFileLabel, newFilePath);
                message.Add(_oldFileLabel, oldFilePath);

                var response = await InteropServiceConnection.SendMessageAsync(message);
                var responseMessage = response.Message;
                if (responseMessage.ContainsKey(_failureLabel) && (bool)responseMessage[_failureLabel])
                {
                    failedFromDesktopExtension = true;
                }
            }

            if (failedFromDesktopExtension)
            {
                throw new Exception("Failed to save due to no Extension access");
            }

            if (failedFromAdminExtension)
            {
                throw new Exception("Failed to save due to no Adminstration access");
            }
        }

        public static async Task CreateElevetedExtension()
        {
            if (InteropServiceConnection == null) return;

            var message = new ValueSet();
            message.Add(_commandLabel, CommandArgs.CreateElevetedExtension.ToString());
            var response = await InteropServiceConnection.SendMessageAsync(message);
        }
    }
}
