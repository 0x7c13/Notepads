namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Microsoft.AppCenter.Analytics;
    using Windows.ApplicationModel;
    using Windows.Foundation.Collections;
    using Windows.Storage;
    using Windows.System;

    public enum NotepadsOperationProtocol
    {
        Unrecognized,
        OpenNewInstance
    }

    public static class NotepadsProtocolService
    {
        private const string NewInstanceProtocolStr = "newinstance";

        public static NotepadsOperationProtocol GetOperationProtocol(Uri uri, out string context)
        {
            context = string.IsNullOrEmpty(uri.Query) ? null : uri.Query.TrimStart('?');

            try
            {
                var uriScheme = uri.GetLeftPart(UriPartial.Scheme);
                if (string.IsNullOrEmpty(uriScheme) || !string.Equals("notepads://", uriScheme, StringComparison.OrdinalIgnoreCase))
                {
                    return NotepadsOperationProtocol.Unrecognized;
                }

                var operation = uri.Authority;
                if (!string.IsNullOrEmpty(operation) && string.Equals(NewInstanceProtocolStr, operation, StringComparison.OrdinalIgnoreCase))
                {
                    return NotepadsOperationProtocol.OpenNewInstance;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsProtocolService)}] Failed to parse protocol: {uri}, exception: {ex}");
                Analytics.TrackEvent("NotepadsProtocolService_FailedToParseProtocol", new Dictionary<string, string>()
                {
                    { "Protocol", uri.ToString() },
                    { "Exception", ex.ToString() }
                });
            }

            return NotepadsOperationProtocol.Unrecognized;
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation, ValueSet message = null)
        {
            try
            {
                if (operation == NotepadsOperationProtocol.Unrecognized)
                {
                    return false;
                }
                else if (operation == NotepadsOperationProtocol.OpenNewInstance)
                {
                    var uri = new Uri($"notepads://{NewInstanceProtocolStr}".ToLower());
                    var launcherOptions = new LauncherOptions() { TargetApplicationPackageFamilyName = Package.Current.Id.FamilyName };
                    return await Launcher.LaunchUriAsync(uri, launcherOptions, message);
                }
                else
                {
                    return false;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsProtocolService)}] Failed to execute protocol: {operation}, Exception: {ex}");
                Analytics.TrackEvent("NotepadsProtocolService_FailedToExecuteProtocol", new Dictionary<string, string>()
                {
                    { "Protocol", operation.ToString() },
                    { "Exception", ex.Message }
                });
                return false;
            }
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation, IStorageFile storageFile)
        {
            try
            {
                if (operation == NotepadsOperationProtocol.OpenNewInstance)
                {
                    var launchOptions = new LauncherOptions() { TargetApplicationPackageFamilyName = Package.Current.Id.FamilyName };
                    return await Launcher.LaunchFileAsync(storageFile, launchOptions);
                }
                else
                {
                    return false;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsProtocolService)}] Failed to execute protocol: {operation}, Exception: {ex}");
                Analytics.TrackEvent("NotepadsProtocolService_FailedToExecuteProtocol", new Dictionary<string, string>()
                {
                    { "Protocol", operation.ToString() },
                    { "Exception", ex.Message }
                });
                return false;
            }
        }
    }
}