namespace Notepads.Commands
{
    using System;

    public class MouseCommand<T> : IMouseCommand<T>
    {
        private readonly bool _ctrl;
        private readonly bool _alt;
        private readonly bool _shift;
        private readonly bool _leftButton;
        private readonly bool _middleButton;
        private readonly bool _rightButton;
        private readonly Action<T> _action;
        private readonly bool _shouldHandle;
        private readonly bool _shouldSwallow;

        public MouseCommand(
            bool leftButtonDown,
            bool middleButtonDown,
            bool rightButtonDown,
            Action<T> action,
            bool shouldHandle = true,
            bool shouldSwallow = true) :
            this(false, false, false, leftButtonDown, middleButtonDown, rightButtonDown, action, shouldHandle, shouldSwallow)
        {
        }

        public MouseCommand(
            bool ctrlDown,
            bool altDown,
            bool shiftDown,
            bool leftButtonDown,
            bool middleButtonDown,
            bool rightButtonDown,
            Action<T> action,
            bool shouldHandle = true,
            bool shouldSwallow = true)
        {
            _ctrl = ctrlDown;
            _alt = altDown;
            _shift = shiftDown;
            _leftButton = leftButtonDown;
            _middleButton = middleButtonDown;
            _rightButton = rightButtonDown;
            _action = action;
            _shouldHandle = shouldHandle;
            _shouldSwallow = shouldSwallow;
        }

        public bool Hit(bool ctrlDown,
            bool altDown,
            bool shiftDown,
            bool leftButtonDown,
            bool middleButtonDown,
            bool rightButtonDown)
        {
            return _ctrl == ctrlDown &&
                   _alt == altDown &&
                   _shift == shiftDown &&
                   _leftButton == leftButtonDown &&
                   _middleButton == middleButtonDown &&
                   _rightButton == rightButtonDown;
        }

        public bool ShouldHandleAfterExecution()
        {
            return _shouldHandle;
        }

        public bool ShouldSwallowAfterExecution()
        {
            return _shouldSwallow;
        }

        public void Execute(T args)
        {
            _action?.Invoke(args);
        }
    }
}