// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Windows.System;

namespace Notepads.Presentation.Input;

/// <summary>
/// Runs <see cref="Action"/> when <see cref="Key"/> is pressed with exactly the Ctrl, Alt and Shift
/// state in <see cref="Modifiers"/>. A handled key stops routing: later handlers do not receive it
/// (for the editor's preview table, neither does the native editor). Every key-down runs the
/// action, including auto-repeat.
/// </summary>
internal sealed record KeyboardCommand(VirtualKeyModifiers Modifiers, VirtualKey Key, Action Action, bool Handled = true);
