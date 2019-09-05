namespace Notepads.Commands
{
    using System;
    using Windows.System;

    public class KeyboardShortcut<T> : IKeyboardCommand<T>
    {
        private static readonly TimeSpan ConsecutiveHitsInterval = TimeSpan.FromMilliseconds(500);

        private readonly bool _ctrl;
        private readonly bool _alt;
        private readonly bool _shift;
        private readonly VirtualKey _key;
        private readonly Action<T> _action;
        private readonly int _requiredHits;
        private int _hits;
        private DateTime _lastHitTimestamp;

        public KeyboardShortcut(VirtualKey key, Action<T> action) : this(false, false, false, key, action)
        {
        }

        public KeyboardShortcut(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key, Action<T> action, int requiredHits = 1)
        {
            _ctrl = ctrlDown;
            _alt = altDown;
            _shift = shiftDown;
            _key = key;
            _action = action;
            _requiredHits = requiredHits;
            _hits = 0;
            _lastHitTimestamp = DateTime.MinValue;
        }

        public bool Hit(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key)
        {
            return _ctrl == ctrlDown && _alt == altDown && _shift == shiftDown && _key == key;
        }

        public bool ShouldExecute(IKeyboardCommand<T> lastCommand)
        {
            DateTime now = DateTime.UtcNow;

            if (lastCommand == this && now - _lastHitTimestamp < ConsecutiveHitsInterval)
            {
                _hits++;
            }
            else
            {
                _hits = 1;
            }

            _lastHitTimestamp = now;

            if (_hits >= _requiredHits)
            {
                _hits = 0;
                return true;
            }

            return false;
        }

        public void Execute(T args)
        {
            _action?.Invoke(args);
        }
    }
}