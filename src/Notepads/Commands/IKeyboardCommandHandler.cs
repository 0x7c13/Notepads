namespace Notepads.Commands
{
    public interface IKeyboardCommandHandler<in T>
    {
        KeyboardCommandHandlerResult Handle(T args);
    }
}