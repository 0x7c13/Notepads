// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Commands
{
    public interface IMouseCommand<in T>
    {
        bool Hit(
            bool ctrlDown,
            bool altDown,
            bool shiftDown,
            bool leftButtonDown,
            bool middleButtonDown,
            bool rightButtonDown);

        bool ShouldHandleAfterExecution();

        bool ShouldSwallowAfterExecution();

        void Execute(T args);
    }
}