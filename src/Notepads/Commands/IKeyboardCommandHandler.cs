
namespace Notepads.Commands
{
    using Windows.System;
    using Windows.UI.Xaml.Input;

    public interface IKeyboardCommandHandler<in T>
    {
        void Handle(T args);
    }
}