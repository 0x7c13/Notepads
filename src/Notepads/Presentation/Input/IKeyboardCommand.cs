// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.System;

namespace Notepads.Presentation.Input;

public interface IKeyboardCommand<T>
{
    bool Hit(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key);

    bool ShouldExecute(IKeyboardCommand<T> lastCommand);

    bool ShouldHandleAfterExecution();

    bool ShouldSwallowAfterExecution();

    void Execute(T args);
}
