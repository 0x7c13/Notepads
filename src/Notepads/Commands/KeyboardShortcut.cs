
namespace Notepads.Commands
{
    using System;
    using Windows.System;

    public class KeyboardShortcut<T> : IKeyboardCommand<T>
    {
        private readonly bool _ctrl;
        private readonly bool _alt;
        private readonly bool _shift;
        private readonly VirtualKey _key;
        private readonly Action<T> _action;

        public KeyboardShortcut(VirtualKey key, Action<T> action) : this(false, false, false, key, action)
        {
        }

        public KeyboardShortcut(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key, Action<T> action)
        {
            _ctrl = ctrlDown;
            _alt = altDown;
            _shift = shiftDown;
            _key = key;
            _action = action;
        }

        public bool Hit(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key)
        {
            return _ctrl == ctrlDown && _alt == altDown && _shift == shiftDown && _key == key;
        }

        public void Execute(T args)
        {
            _action?.Invoke(args);
        }
    }
}
