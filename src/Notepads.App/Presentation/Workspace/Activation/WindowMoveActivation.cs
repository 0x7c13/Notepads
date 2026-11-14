// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text.Json;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.Foundation.Collections;
using Windows.System;
using WinRT;

namespace Notepads.Presentation.Workspace.Activation;

internal static class WindowMoveActivation
{
    internal const string DataKey = "Notepads.WindowMove.v1";

    internal static TransferToken Read(IActivatedEventArgs activation, string scheme)
    {
        if (activation.Kind != ActivationKind.Protocol) return null;
        var uri = activation.As<IProtocolActivatedEventArgs>().Uri;
        if (!string.Equals(uri.Scheme, scheme, StringComparison.OrdinalIgnoreCase) ||
            !string.Equals(uri.Host, "movetab", StringComparison.OrdinalIgnoreCase)) return null;
        var caller = activation.As<IProtocolActivatedEventArgsWithCallerPackageFamilyNameAndData>();
        if (caller.CallerPackageFamilyName != Package.Current.Id.FamilyName || caller.Data?.Count != 1 ||
            !caller.Data.TryGetValue(DataKey, out var data) || data is not string json || json.Length > 2048 ||
            !string.IsNullOrEmpty(uri.UserInfo) || !uri.IsDefaultPort || !string.IsNullOrEmpty(uri.Query) ||
            !string.IsNullOrEmpty(uri.Fragment) || !Guid.TryParseExact(uri.AbsolutePath.AsSpan(1), "N", out var id))
            return null;
        try
        {
            var token = JsonSerializer.Deserialize(json, SessionJsonContext.Default.TransferToken);
            token?.Validate();
            return token?.TransferId == id ? token : null;
        }
        catch (Exception error) when (error is JsonException or InvalidDataException) { return null; }
    }

    internal static async Task<bool> LaunchAsync(TransferToken token, string scheme)
    {
        token.Validate();
        var options = new LauncherOptions { TargetApplicationPackageFamilyName = Package.Current.Id.FamilyName };
        var data = new ValueSet { [DataKey] = JsonSerializer.Serialize(token, SessionJsonContext.Default.TransferToken) };
        return await Launcher.LaunchUriAsync(new Uri($"{scheme}://movetab/{token.TransferId:N}"), options, data);
    }
}
