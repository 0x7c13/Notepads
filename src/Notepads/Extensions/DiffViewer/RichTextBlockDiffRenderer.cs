
namespace Notepads.Extensions.DiffViewer
{
    using DiffPlex;
    using DiffPlex.DiffBuilder;
    using DiffPlex.DiffBuilder.Model;
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Documents;
    using Windows.UI.Xaml.Media;

    public static class BrushFactory
    {
        public static Dictionary<Color, Brush> Brushes = new Dictionary<Color, Brush>();

        public static Brush GetSolidColorBrush(Color color)
        {
            if (Brushes.ContainsKey(color))
            {
                return Brushes[color];
            }
            else
            {
                Brushes[color] = new SolidColorBrush(color);
                return Brushes[color];
            }
        }
    }

    public class RichTextBlockDiffContext
    {
        private bool _hasPendingHightlighter;
        private int _lastStart;
        private int _lastEnd;
        private Color _lastHightlightColor;

        public RichTextBlockDiffContext()
        {
            Blocks = new List<Block>();
            TextHighlighters = new List<TextHighlighter>();
        }

        public IList<Block> Blocks { get; set; }

        private IList<TextHighlighter> TextHighlighters { get; set; }

        public void QueuePendingHightlighter(TextRange textRange, Color backgroundColor)
        {
            _lastStart = textRange.StartIndex;
            _lastEnd = _lastStart + textRange.Length;
            _lastHightlightColor = backgroundColor;
            _hasPendingHightlighter = true;
        }

        public void AddTextHighlighter(TextRange textRange, Color backgroundColor)
        {
            if (!_hasPendingHightlighter)
            {
                QueuePendingHightlighter(textRange, backgroundColor);
            }
            else
            {
                if (_lastEnd == textRange.StartIndex && _lastHightlightColor == backgroundColor)
                {
                    _lastEnd += textRange.Length;
                }
                else
                {
                    TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                    TextHighlighters.Add(new TextHighlighter()
                    {
                        Background = BrushFactory.GetSolidColorBrush(_lastHightlightColor),
                        Ranges = { range }
                    });
                    QueuePendingHightlighter(textRange, backgroundColor);
                }
            }
        }

        public IList<TextHighlighter> GetTextHighlighters()
        {
            if (_hasPendingHightlighter)
            {
                TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                TextHighlighters.Add(new TextHighlighter()
                {
                    Background = BrushFactory.GetSolidColorBrush(_lastHightlightColor),
                    Ranges = { range }
                });
                _hasPendingHightlighter = false;
            }
            return TextHighlighters;
        }
    }

    public class RichTextBlockDiffRenderer
    {
        //private const char ImaginaryLineCharacter = '\u202B';
        private readonly SideBySideDiffBuilder differ;
        private readonly object mutex = new object();
        private bool inDiff;

        public RichTextBlockDiffRenderer()
        {
            differ = new SideBySideDiffBuilder(new Differ());
        }

        private static readonly char BreakingSpace = '-';
        private Brush _defaultForeground;

        public Tuple<RichTextBlockDiffContext, RichTextBlockDiffContext> GenerateDiffViewData(string leftText, string rightText, Brush defaultForeground)
        {
            if (inDiff)
            {
                return null;
            }

            lock (mutex)
            {
                if (inDiff)
                {
                    return null;
                }

                inDiff = true;
            }

            _defaultForeground = defaultForeground;

            SideBySideDiffModel diff = differ.BuildDiffModel(leftText, rightText, ignoreWhitespace: false);
            List<OldNew<DiffPiece>> zippedDiffs = Enumerable.Zip(diff.OldText.Lines, diff.NewText.Lines, (oldLine, newLine) => new OldNew<DiffPiece> { Old = oldLine, New = newLine }).ToList();
            RichTextBlockDiffContext leftContext = GetDiffData(zippedDiffs, line => line.Old, piece => piece.Old);
            RichTextBlockDiffContext rightContext = GetDiffData(zippedDiffs, line => line.New, piece => piece.New);

            inDiff = false;
            return new Tuple<RichTextBlockDiffContext, RichTextBlockDiffContext>(leftContext, rightContext);
        }

        private RichTextBlockDiffContext GetDiffData(System.Collections.Generic.List<OldNew<DiffPiece>> lines, Func<OldNew<DiffPiece>, DiffPiece> lineSelector, Func<OldNew<DiffPiece>, DiffPiece> pieceSelector)
        {
            RichTextBlockDiffContext context = new RichTextBlockDiffContext();
            int pointer = 0;
            foreach (OldNew<DiffPiece> line in lines)
            {
                int synchroLineLength = Math.Max(line.Old.Text?.Length ?? 0, line.New.Text?.Length ?? 0);
                IEnumerable<OldNew<DiffPiece>> lineSubPieces = Enumerable.Zip(line.Old.SubPieces, line.New.SubPieces, (oldPiece, newPiece) => new OldNew<DiffPiece> { Old = oldPiece, New = newPiece, Length = Math.Max(oldPiece.Text?.Length ?? 0, newPiece.Text?.Length ?? 0) });

                DiffPiece oldNewLine = lineSelector(line);
                switch (oldNewLine.Type)
                {
                    case ChangeType.Unchanged:
                        AppendParagraph(context, oldNewLine.Text ?? string.Empty, ref pointer, null);
                        break;
                    case ChangeType.Imaginary:
                        AppendParagraph(context, new string(BreakingSpace, synchroLineLength), ref pointer, Colors.Gray, Colors.LightCyan);
                        break;
                    case ChangeType.Inserted:
                        AppendParagraph(context, oldNewLine.Text ?? string.Empty, ref pointer, Colors.LightGreen);
                        break;
                    case ChangeType.Deleted:
                        AppendParagraph(context, oldNewLine.Text ?? string.Empty, ref pointer, Colors.OrangeRed);
                        break;
                    case ChangeType.Modified:
                        Paragraph paragraph = new Paragraph()
                        {
                            LineStackingStrategy = LineStackingStrategy.BlockLineHeight,
                            Foreground = _defaultForeground,
                        };
                        paragraph.LineHeight = paragraph.FontSize + 6;
                        foreach (OldNew<DiffPiece> subPiece in lineSubPieces)
                        {
                            DiffPiece oldNewPiece = pieceSelector(subPiece);
                            switch (oldNewPiece.Type)
                            {
                                case ChangeType.Unchanged: paragraph.Inlines.Add(NewRun(context, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.Yellow)); break;
                                case ChangeType.Imaginary: paragraph.Inlines.Add(NewRun(context, oldNewPiece.Text ?? string.Empty, ref pointer)); break;
                                case ChangeType.Inserted: paragraph.Inlines.Add(NewRun(context, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.LightGreen)); break;
                                case ChangeType.Deleted: paragraph.Inlines.Add(NewRun(context, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.OrangeRed)); break;
                                case ChangeType.Modified: paragraph.Inlines.Add(NewRun(context, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.Yellow)); break;
                            }
                            paragraph.Inlines.Add(NewRun(context, new string(BreakingSpace, subPiece.Length - (oldNewPiece.Text ?? string.Empty).Length), ref pointer, Colors.Gray, Colors.LightCyan));
                        }
                        context.Blocks.Add(paragraph);
                        break;
                }
            }
            return context;
        }

        private Inline NewRun(RichTextBlockDiffContext richTextBlockData, string text, ref int pointer, Color? background = null, Color? foreground = null)
        {
            Run run = new Run
            {
                Text = text,
                Foreground = foreground.HasValue
                    ? BrushFactory.GetSolidColorBrush(foreground.Value)
                    : _defaultForeground
            };

            if (background != null)
            {
                richTextBlockData.AddTextHighlighter(new TextRange() { StartIndex = pointer, Length = text.Length }, background.Value);
            }
            pointer += text.Length;
            return run;
        }

        private Paragraph AppendParagraph(RichTextBlockDiffContext richTextBlockData, string text, ref int pointer, Color? background = null, Color? foreground = null)
        {
            Paragraph paragraph = new Paragraph
            {
                LineStackingStrategy = LineStackingStrategy.BlockLineHeight,
                Foreground = foreground.HasValue
                    ? BrushFactory.GetSolidColorBrush(foreground.Value)
                    : _defaultForeground,
            };
            paragraph.LineHeight = paragraph.FontSize + 6;

            Run run = new Run { Text = text };
            paragraph.Inlines.Add(run);

            richTextBlockData.Blocks.Add(paragraph);

            if (background != null)
            {
                richTextBlockData.AddTextHighlighter(new TextRange() { StartIndex = pointer, Length = text.Length }, background.Value);
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