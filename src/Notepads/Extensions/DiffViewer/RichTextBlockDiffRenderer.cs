
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

        public ICollection<Block> Blocks { get; set; }

        private IList<TextHighlighter> TextHighlighters { get; set; }

        public void AddPendingHightlighter(TextRange textRange, Color backgroundColor)
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
                AddPendingHightlighter(textRange, backgroundColor);
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
                        Background = new SolidColorBrush(_lastHightlightColor),
                        Ranges = { range }
                    });
                    _hasPendingHightlighter = false;
                    AddPendingHightlighter(textRange, backgroundColor);
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
                    Background = new SolidColorBrush(_lastHightlightColor),
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
            if (inDiff) return null;
            lock (mutex)
            {
                if (inDiff) return null;
                inDiff = true;
            }

            _defaultForeground = defaultForeground;

            var diff = differ.BuildDiffModel(leftText, rightText);
            var zippedDiffs = Enumerable.Zip(diff.OldText.Lines, diff.NewText.Lines, (oldLine, newLine) => new OldNew<DiffPiece> { Old = oldLine, New = newLine }).ToList();
            var leftRichTextBlockData = GetDiffData(zippedDiffs, line => line.Old, piece => piece.Old);
            var rightRichTextBlockData = GetDiffData(zippedDiffs, line => line.New, piece => piece.New);

            inDiff = false;
            return new Tuple<RichTextBlockDiffContext, RichTextBlockDiffContext>(leftRichTextBlockData, rightRichTextBlockData);
        }

        private RichTextBlockDiffContext GetDiffData(System.Collections.Generic.List<OldNew<DiffPiece>> lines, Func<OldNew<DiffPiece>, DiffPiece> lineSelector, Func<OldNew<DiffPiece>, DiffPiece> pieceSelector)
        {
            var data = new RichTextBlockDiffContext();
            int pointer = 0;
            foreach (var line in lines)
            {
                var synchroLineLength = Math.Max(line.Old.Text?.Length ?? 0, line.New.Text?.Length ?? 0);
                var lineSubPieces = Enumerable.Zip(line.Old.SubPieces, line.New.SubPieces, (oldPiece, newPiece) => new OldNew<DiffPiece> { Old = oldPiece, New = newPiece, Length = Math.Max(oldPiece.Text?.Length ?? 0, newPiece.Text?.Length ?? 0) });

                var oldNewLine = lineSelector(line);
                switch (oldNewLine.Type)
                {
                    case ChangeType.Unchanged:
                        AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer, null);
                        break;
                    case ChangeType.Imaginary:
                        AppendParagraph(data, new string(BreakingSpace, synchroLineLength), ref pointer, Colors.Gray, Colors.LightCyan);
                        break;
                    case ChangeType.Inserted:
                        AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer, Colors.LightGreen);
                        break;
                    case ChangeType.Deleted:
                        AppendParagraph(data, oldNewLine.Text ?? string.Empty, ref pointer, Colors.OrangeRed);
                        break;
                    case ChangeType.Modified:
                        var paragraph = new Paragraph()
                        {
                            LineHeight = 0.5,
                            Foreground = _defaultForeground,
                        };
                        foreach (var subPiece in lineSubPieces)
                        {
                            var oldNewPiece = pieceSelector(subPiece);
                            switch (oldNewPiece.Type)
                            {
                                case ChangeType.Unchanged: paragraph.Inlines.Add(NewRun(data, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.Yellow)); break;
                                case ChangeType.Imaginary: paragraph.Inlines.Add(NewRun(data, oldNewPiece.Text ?? string.Empty, ref pointer)); break;
                                case ChangeType.Inserted: paragraph.Inlines.Add(NewRun(data, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.LightGreen)); break;
                                case ChangeType.Deleted: paragraph.Inlines.Add(NewRun(data, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.OrangeRed)); break;
                                case ChangeType.Modified: paragraph.Inlines.Add(NewRun(data, oldNewPiece.Text ?? string.Empty, ref pointer, Colors.Yellow)); break;
                            }
                            paragraph.Inlines.Add(NewRun(data, new string(BreakingSpace, subPiece.Length - (oldNewPiece.Text ?? string.Empty).Length), ref pointer, Colors.Gray, Colors.LightCyan));
                        }
                        data.Blocks.Add(paragraph);
                        break;
                }
            }
            return data;
        }

        private Inline NewRun(RichTextBlockDiffContext richTextBlockData, string text, ref int pointer, Color? background = null, Color? foreground = null)
        {
            var run = new Run { Text = text };

            if (foreground.HasValue)
            {
                run.Foreground = new SolidColorBrush(foreground.Value);
            }
            else
            {
                run.Foreground = _defaultForeground;
            }

            if (background != null)
            {
                richTextBlockData.AddTextHighlighter(new TextRange() { StartIndex = pointer, Length = text.Length }, background.Value);
            }
            pointer += text.Length;
            return run;
        }

        private Paragraph AppendParagraph(RichTextBlockDiffContext richTextBlockData, string text, ref int pointer, Color? background = null, Color? foreground = null)
        {
            var paragraph = new Paragraph()
            {
                LineHeight = 0.5,
            };

            if (foreground.HasValue)
            {
                paragraph.Foreground = new SolidColorBrush(foreground.Value);
            }
            else
            {
                paragraph.Foreground = _defaultForeground;
            }

            var run = new Run { Text = text };
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