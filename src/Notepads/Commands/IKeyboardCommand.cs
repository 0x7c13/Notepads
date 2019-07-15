
namespace Notepads.Commands
{
    using Windows.System;
    using Windows.UI.Xaml.Input;

    public interface IKeyboardCommand<in T>
    {
        bool Hit(bool ctrlDown, bool altDown, bool shiftDown, VirtualKey key);

        void Execute(T args);
    }
}
