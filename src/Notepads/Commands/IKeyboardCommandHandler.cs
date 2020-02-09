namespace Notepads.Commands
{
    public interface IKeyboardCommandHandler<in T>
    {
        void Handle(T args);
    }
}