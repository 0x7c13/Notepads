// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Presentation.Workspace;

namespace Notepads.Presentation.Views.Settings;

internal sealed class SettingsContext
{
    public SettingsContext(WindowContext window)
    {
        Window = window ?? throw new ArgumentNullException(nameof(window));
    }
    public WindowContext Window { get; }
    public string ApplicationName => Window.ApplicationName;
}
