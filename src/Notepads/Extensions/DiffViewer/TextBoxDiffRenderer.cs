
namespace Notepads.Extensions.DiffViewer
{
    using DiffPlex;
    using DiffPlex.DiffBuilder;
    using DiffPlex.DiffBuilder.Model;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Documents;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Shapes;

    public class RichTextBlockData
    {
        public RichTextBlockData()
        {
            Blocks = new List<Block>();
            TextHighlighters = new List<TextHighlighter>();
        }

        public ICollection<Block> Blocks { get; set; }

        public IList<TextHighlighter> TextHighlighters { get; set; }
    }

    public class TextBoxDiffRenderer
    {
        private const char ImaginaryLineCharacter = '\u202B';
        private readonly SideBySideDiffBuilder differ;
        private readonly object mutex = new object();
        private bool inDiff;

        public TextBoxDiffRenderer()
        {
            differ = new SideBySideDiffBuilder(new Differ());
        }

        private static char BreakingSpace = '-';

        public Tuple<RichTextBlockData, RichTextBlockData> GenerateDiffViewData(string leftText, string rightText)
        {
            if (inDiff) return null;
            lock (mutex)
            {
                if (inDiff) return null;
                inDiff = true;
            }

            var diff = differ.BuildDiffModel(leftText, rightText);
            var zippedDiffs = Enumerable.Zip(diff.OldText.Lines, diff.NewText.Lines, (oldLine, newLine) => new OldNew<DiffPiece> { Old = oldLine, New = newLine }).ToList();
            var leftRichTextBlockData = GetDiffData(zippedDiffs, line => line.Old, piece => piece.Old);
            var rightRichTextBlockData = GetDiffData(zippedDiffs, line => line.New, piece => piece.New);

            inDiff = false;
            return new Tuple<RichTextBlockData, RichTextBlockData>(leftRichTextBlockData, rightRichTextBlockData);
        }

        private RichTextBlockData GetDiffData(System.Collections.Generic.List<OldNew<DiffPiece>> lines, Func<OldNew<DiffPiece>, DiffPiece> lineSelector, Func<OldNew<DiffPiece>, DiffPiece> pieceSelector)
        {
            var data = new RichTextBlockData();
            int pointer = 0;
            foreach (var line in lines)
            {
                var synchroLineLength = Math.Max(line.Old.Text?.Length ?? 0, line.New.Text?.Length ?? 0);
                var lineSubPieces = Enumerable.Zip(line.Old.SubPieces, line.New.SubPieces, (oldPiece, newPiece) => new OldNew<DiffPiece> { Old = oldPiece, New = newPiece, Length = Math.Max(oldPiece.Text?.Length ?? 0, newPiece.Text?.Length ?? 0) });

                var oldNewLine = lineSelector(line);
                switch (oldNewLine.Type)
                {
                    case ChangeType.Unchanged: AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer); break;
                    case ChangeType.Imaginary: AppendParagraph(data, new string(BreakingSpace, synchroLineLength), ref pointer, new SolidColorBrush(Colors.Gray), new SolidColorBrush(Colors.LightCyan)); break;
                    case ChangeType.Inserted: AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer, new SolidColorBrush(Colors.LightGreen)); break;
                    case ChangeType.Deleted: AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer, new SolidColorBrush(Colors.OrangeRed)); break;
                    case ChangeType.Modified:
                        //var paragraph = AppendParagraph(diffBox, string.Empty);
                        //foreach (var subPiece in lineSubPieces)
                        //{
                        //    var oldNewPiece = pieceSelector(subPiece);
                        //    switch (oldNewPiece.Type)
                        //    {
                        //        case ChangeType.Unchanged: paragraph.Inlines.Add(NewRun(oldNewPiece.Text ?? string.Empty, Brushes.Yellow)); break;
                        //        case ChangeType.Imaginary: paragraph.Inlines.Add(NewRun(oldNewPiece.Text ?? string.Empty)); break;
                        //        case ChangeType.Inserted: paragraph.Inlines.Add(NewRun(oldNewPiece.Text ?? string.Empty, Brushes.LightGreen)); break;
                        //        case ChangeType.Deleted: paragraph.Inlines.Add(NewRun(oldNewPiece.Text ?? string.Empty, Brushes.OrangeRed)); break;
                        //        case ChangeType.Modified: paragraph.Inlines.Add(NewRun(oldNewPiece.Text ?? string.Empty, Brushes.Yellow)); break;
                        //        default: throw new ArgumentException();
                        //    }
                        //    paragraph.Inlines.Add(NewRun(new string(BreakingSpace, subPiece.Length - (oldNewPiece.Text ?? string.Empty).Length), Brushes.Gray, Brushes.LightCyan));
                        //}
                        break;
                    default: throw new ArgumentException();
                }
            }
            return data;
        }

        private Paragraph AppendParagraph(RichTextBlockData richTextBlockData, string text, ref int pointer, Brush background = null, Brush foreground = null)
        {
            var paragraph = new Paragraph()
            {
                LineHeight = 0.5,
                Foreground = foreground ?? new SolidColorBrush(Colors.White),
            };

            var run = new Run { Text = text };
            paragraph.Inlines.Add(run);

            richTextBlockData.Blocks.Add(paragraph);

            if (background != null)
            {
                TextRange textRange = new TextRange() { StartIndex = pointer, Length = text.Length };
                TextHighlighter highlighter = new TextHighlighter()
                {
                    Background = background,
                    Ranges = { textRange }
                };
                //add the highlighter
                richTextBlockData.TextHighlighters.Add(highlighter);
            }

            pointer += text.Length;
            return paragraph;
        }

        private class OldNew<T>
        {
            public T Old { get; set; }
            public T New { get; set; }
            public int Length { get; set; }
        }
    }
}