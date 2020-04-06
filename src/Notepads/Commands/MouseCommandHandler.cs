namespace Notepads.Commands
{
    using System.Collections.Generic;
    using Windows.System;
    using Windows.UI.Core;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Input;

    public class MouseCommandHandler : ICommandHandler<PointerRoutedEventArgs>
    {
        public readonly ICollection<IMouseCommand<PointerRoutedEventArgs>> Commands;

        private readonly UIElement _relativeTo;

        public MouseCommandHandler(ICollection<IMouseCommand<PointerRoutedEventArgs>> commands, UIElement relativeTo)
        {
            Commands = commands;
            _relativeTo = relativeTo;
        }

        public CommandHandlerResult Handle(PointerRoutedEventArgs args)
        {
            var ctrlDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
            var altDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down);
            var shiftDown = Window.Current.CoreWindow.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down);
            var point = args.GetCurrentPoint(_relativeTo).Properties;
            var shouldHandle = false;
            var shouldSwallow = false;

            foreach (var command in Commands)
            {
                if (command.Hit(
                    ctrlDown,
                    altDown,
                    shiftDown,
                    point.IsLeftButtonPressed,
                    point.IsMiddleButtonPressed,
                    point.IsRightButtonPressed))
                {
                    command.Execute(args);

                    if (command.ShouldSwallowAfterExecution())
                    {
                        shouldSwallow = true;
                    }

                    if (command.ShouldHandleAfterExecution())
                    {
                        shouldHandle = true;
                    }

                    break;
                }
            }

            return new CommandHandlerResult(shouldHandle, shouldSwallow);
        }
    }
}