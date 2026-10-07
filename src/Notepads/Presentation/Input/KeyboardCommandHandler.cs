// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.System;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Input;

internal sealed class KeyboardCommandHandler
{
    private readonly KeyboardCommand[] _commands;

    public KeyboardCommandHandler(KeyboardCommand[] commands)
    {
        _commands = commands;
    }

    /// <summary>
    /// Runs the first command that matches the key and the current Ctrl, Alt and Shift state.
    /// Returns whether the caller should mark the key handled.
    /// </summary>
    public bool Handle(KeyRoutedEventArgs args)
    {
        var window = Window.Current.CoreWindow;
        var modifiers = VirtualKeyModifiers.None;
        if (window.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Control;
        if (window.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Menu;
        if (window.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down)) modifiers |= VirtualKeyModifiers.Shift;

        foreach (var command in _commands)
        {
            if (command.Key == args.Key && command.Modifiers == modifiers)
            {
                command.Action();
                return command.Handled;
            }
        }

        return false;
    }
}
