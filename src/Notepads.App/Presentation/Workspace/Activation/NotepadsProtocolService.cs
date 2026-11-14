// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.ApplicationModel;
using Windows.System;

namespace Notepads.Presentation.Workspace.Activation;

public enum NotepadsOperationProtocol
{
    Unrecognized,
    OpenNewInstance
}

public static class NotepadsProtocolService
{
    private const string NewInstanceProtocolStr = "newinstance";

    public static NotepadsOperationProtocol GetOperationProtocol(Uri uri)
    {
        try
        {
            var uriStr = uri.ToString().Trim();

            if (string.IsNullOrEmpty(uriStr) || !uriStr.StartsWith("notepads://", StringComparison.OrdinalIgnoreCase))
            {
                return NotepadsOperationProtocol.Unrecognized;
            }

            var operation = uriStr.AsSpan("notepads://".Length);

            if (operation.EndsWith("/", StringComparison.Ordinal))
            {
                operation = operation[..^1];
            }

            if (operation.Equals(NewInstanceProtocolStr, StringComparison.OrdinalIgnoreCase))
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
