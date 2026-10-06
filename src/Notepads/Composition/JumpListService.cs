// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Settings;
using Notepads.Infrastructure.Shell;
using Windows.ApplicationModel;
using Windows.UI.StartScreen;

namespace Notepads.Composition;

public static class JumpListService
{
    public static bool IsJumpListOutOfDate
    {
        get
        {
            if (ApplicationSettingsStore.Read(SettingsKey.IsJumpListOutOfDateBool) is bool isJumpListOutOfDate)
            {
                return isJumpListOutOfDate;
            }

            return true;
        }
        set => ApplicationSettingsStore.Write(SettingsKey.IsJumpListOutOfDateBool, value);
    }

    public static Task<bool> UpdateJumpListAsync() => JumpListAdapter.UpdateJumpListAsync(new[] { GetNewWindowItem() });

    public static Task<bool> ClearJumpListAsync() => JumpListAdapter.ClearJumpListAsync();

    private static JumpListItem GetNewWindowItem()
    {
        string packageId = Package.Current.Id.Name;
        var item = JumpListItem.CreateWithArguments("notepads://newinstance", $"ms-resource://{packageId}/Resources/JumpList_Tasks_NewWindow_Title");
        item.Description = $"ms-resource://{packageId}/Resources/JumpList_Tasks_NewWindow_Description";
        item.Logo = new Uri($"ms-appx:///Assets/Square44x44Logo.png");
        return item;
    }
}
