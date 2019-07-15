
namespace Notepads.Commands
{
    using System.Collections.Generic;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Input;

    public class KeyboardCommandHandler : IKeyboardCommandHandler<KeyRoutedEventArgs>
    {
        public readonly ICollection<IKeyboardCommand<KeyRoutedEventArgs>> Commands;

        public KeyboardCommandHandler(ICollection<IKeyboardCommand<KeyRoutedEventArgs>> commands)
        {
            Commands = commands;
        }

        public void Handle(KeyRoutedEventArgs args)
        {
            var ctrlDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
            var altDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down);
            var shiftDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down);

            foreach (var keyboardCommand in Commands)
            {
                if (keyboardCommand.Hit(ctrlDown, altDown, shiftDown, args.Key))
                {
                    args.Handled = true;
                    keyboardCommand.Execute(args);
                }
            }
        }
    }
}
