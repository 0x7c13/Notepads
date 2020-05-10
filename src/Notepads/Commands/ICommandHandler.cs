namespace Notepads.Commands
{
    public interface ICommandHandler<in T>
    {
        CommandHandlerResult Handle(T args);
    }
}