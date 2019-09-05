namespace Notepads.Extensions.DiffViewer
{
    public interface ISideBySideDiffViewer
    {
        void RenderDiff(string left, string right);
    }
}