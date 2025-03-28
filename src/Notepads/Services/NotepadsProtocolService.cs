// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Services
{
    using System;
    using System.Collections.Generic;
    using System.Threading.Tasks;
    using Windows.ApplicationModel;
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
            context = null;

            try
            {
                var uriStr = uri.ToString().Trim();

                if (string.IsNullOrEmpty(uriStr) || !uriStr.StartsWith("notepads://", StringComparison.OrdinalIgnoreCase))
                {
                    return NotepadsOperationProtocol.Unrecognized;
                }

                var operation = uriStr.Substring("notepads://".Length);

                if (operation.EndsWith("/"))
                {
                    operation = operation.Remove(operation.Length - 1);
                }

                if (!string.IsNullOrEmpty(operation) && string.Equals(NewInstanceProtocolStr, operation, StringComparison.OrdinalIgnoreCase))
                {
                    return NotepadsOperationProtocol.OpenNewInstance;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsProtocolService)}] Failed to parse protocol: {uri}, exception: {ex}");
                AnalyticsService.TrackEvent("NotepadsProtocolService_FailedToParseProtocol", new Dictionary<string, string>()
                {
                    { "Protocol", uri.ToString() },
                    { "Exception", ex.ToString() }
                });
            }

            return NotepadsOperationProtocol.Unrecognized;
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation)
        {
            try
            {
                if (operation == NotepadsOperationProtocol.Unrecognized)
                {
                    return false;
                }
                else if (operation == NotepadsOperationProtocol.OpenNewInstance)
                {
                    var uriToLaunch = $"notepads://{NewInstanceProtocolStr}";
                    var launchOptions = new LauncherOptions { TargetApplicationPackageFamilyName = Package.Current.Id.FamilyName };
                    return await Launcher.LaunchUriAsync(new Uri(uriToLaunch.ToLower()), launchOptions);
                }
                else
                {
                    return false;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(NotepadsProtocolService)}] Failed to execute protocol: {operation}, Exception: {ex}");
                AnalyticsService.TrackEvent("NotepadsProtocolService_FailedToExecuteProtocol", new Dictionary<string, string>()
                {
                    { "Protocol", operation.ToString() },
                    { "Exception", ex.Message }
                });
                return false;
            }
        }
    }
}