// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Linq;
using System.Threading.Tasks;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Views.MainPage;

public sealed partial class NotepadsMainPage
{
    internal Task TestOpenPickerAsync() => OpenNewFilesAsync();
    internal Canvas TestPrintCanvas => PrintCanvas;
    internal Task TestBuildOpenRecentAsync() => BuildOpenRecentButtonSubItemsAsync();
    internal MenuFlyoutSubItem TestOpenRecentMenu => MainMenuButtonFlyout.Items.OfType<MenuFlyoutSubItem>()
        .Single(item => item.Name == "MenuOpenRecentlyUsedFileButton");
}
