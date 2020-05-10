namespace Notepads.Commands
{
    public interface IMouseCommand<in T>
    {
        bool Hit(
            bool ctrlDown,
            bool altDown,
            bool shiftDown,
            bool leftButtonDown,
            bool middleButtonDown,
            bool rightButtonDown);

        bool ShouldHandleAfterExecution();

        bool ShouldSwallowAfterExecution();

        void Execute(T args);
    }
}