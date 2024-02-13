// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Commands
{
    using Windows.System;

    public interface IKeyboardCommand<T>
    {
        bool Hit(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key);

        bool ShouldExecute(IKeyboardCommand<T> lastCommand);

        bool ShouldHandleAfterExecution();

        bool ShouldSwallowAfterExecution();

        void Execute(T args);
    }
}