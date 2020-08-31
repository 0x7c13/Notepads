namespace Notepads.Models
{
    using System.Text;
    using Notepads.Utilities;

    public class TextFile
    {
        public TextFile(string content, Encoding encoding, LineEnding lineEnding, long dateModifiedFileTime = -1)
        {
            Content = content;
            Encoding = encoding;
            LineEnding = lineEnding;
            DateModifiedFileTime = dateModifiedFileTime;
        }

        public string Content { get; set; }

        public Encoding Encoding { get; set; }

        public LineEnding LineEnding { get; set; }

        public long DateModifiedFileTime { get; set; }
    }
}