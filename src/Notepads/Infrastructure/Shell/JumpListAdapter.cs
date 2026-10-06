// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.UI.StartScreen;

namespace Notepads.Infrastructure.Shell;

internal static class JumpListAdapter
{
    public static async Task<bool> UpdateJumpListAsync(IEnumerable<JumpListItem> items)
    {
        if (!JumpList.IsSupported()) return false;

        try
        {
            JumpList jumpList = await JumpList.LoadCurrentAsync();
            jumpList.Items.Clear();
            jumpList.SystemGroupKind = JumpListSystemGroupKind.None;

            foreach (var item in items) jumpList.Items.Add(item);

            await jumpList.SaveAsync();

            return true;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(JumpListAdapter)}] FailedToSetupJumpList: {ex.Message}");
        }

        return false;
    }

    public static async Task<bool> ClearJumpListAsync()
    {
        if (!JumpList.IsSupported()) return false;

        try
        {
            JumpList jumpList = await JumpList.LoadCurrentAsync();
            jumpList.Items.Clear();
            return true;
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(JumpListAdapter)}] FailedToClearJumpList: {ex.Message}");
        }

        return false;
    }

}
