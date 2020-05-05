namespace Notepads.Controls.TextEditor
{
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    public enum ScrollBarOrientation
    {
        Horizontal,
        Vertical
    }
    public enum ScrollCommand
    {
        ScrollTo,
        Top,
        Bottom,
        PageUp,
        PageDown,
        LeftEdge,
        RightEdge,
        PageLeft,
        PageRight
    }

    class TextEditorScrollBarContextFlyout : MenuFlyout
    {
        private MenuFlyoutItem _scrollHere;
        private MenuFlyoutItem _top;
        private MenuFlyoutItem _bottom;
        private MenuFlyoutItem _pageUp;
        private MenuFlyoutItem _pageDown;
        private MenuFlyoutItem _leftEdge;
        private MenuFlyoutItem _rightEdge;
        private MenuFlyoutItem _pageLeft;
        private MenuFlyoutItem _pageRight;

        private readonly ScrollBarOrientation _scrollBarOrientation;
        private readonly TextEditorCore _textEditorCore;

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public double ClickLocation;

        public TextEditorScrollBarContextFlyout(TextEditorCore textEditorCore, ScrollBarOrientation scrollBarOrientation)
        {
            _textEditorCore = textEditorCore;
            _scrollBarOrientation = scrollBarOrientation;

            Items.Add(ScrollHere);
            Items.Add(new MenuFlyoutSeparator());
            switch (scrollBarOrientation)
            {
                case ScrollBarOrientation.Horizontal:
                    Items.Add(LeftEdge);
                    Items.Add(RightEdge);
                    Items.Add(new MenuFlyoutSeparator());
                    Items.Add(PageLeft);
                    Items.Add(PageRight);
                    break;
                case ScrollBarOrientation.Vertical:
                    Items.Add(Top);
                    Items.Add(Bottom);
                    Items.Add(new MenuFlyoutSeparator());
                    Items.Add(PageUp);
                    Items.Add(PageDown);
                    break;
            }
            
            Opening += TextEditorVerticalScrollBarFlyout_Opening;
        }

        public void Dispose()
        {
            Opening -= TextEditorVerticalScrollBarFlyout_Opening;
        }

        private void TextEditorVerticalScrollBarFlyout_Opening(object sender, object e)
        {
            return;
        }

        public MenuFlyoutItem ScrollHere
        {
            get
            {
                if (_scrollHere == null)
                {
                    _scrollHere = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_ScrollHereButtonDisplayText") };
                    _scrollHere.Click += (sender, args) => { _textEditorCore.SetScrollView(ScrollCommand.ScrollTo, _scrollBarOrientation, ClickLocation); };
                }
                return _scrollHere;
            }
        }

        public MenuFlyoutItem Top
        {
            get
            {
                if (_top == null)
                {
                    _top = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_TopButtonDisplayText") };
                    _top.Click += (sender, args) => { _textEditorCore.SetScrollView(ScrollCommand.Top, _scrollBarOrientation); };
                }
                return _top;
            }
        }

        public MenuFlyoutItem Bottom
        {
            get
            {
                if (_bottom == null)
                {
                    _bottom = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_BottomButtonDisplayText") };
                    _bottom.Click += (sender, args) => { _textEditorCore.SetScrollView(ScrollCommand.Bottom, _scrollBarOrientation); };
                }
                return _bottom;
            }
        }

        public MenuFlyoutItem PageUp
        {
            get
            {
                if (_pageUp == null)
                {
                    _pageUp = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_PageUpButtonDisplayText") };
                    _pageUp.Click += (sender, args) => { _textEditorCore.SetScrollView(ScrollCommand.PageUp, _scrollBarOrientation); };
                }
                return _pageUp;
            }
        }

        public MenuFlyoutItem PageDown
        {
            get
            {
                if (_pageDown == null)
                {
                    _pageDown = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_PageDownButtonDisplayText") };
                    _pageDown.Click += (sender, args) => { _textEditorCore.SetScrollView(ScrollCommand.PageDown, _scrollBarOrientation); };
                }
                return _pageDown;
            }
        }

        public MenuFlyoutItem LeftEdge
        {
            get
            {
                if (_leftEdge == null)
                {
                    _leftEdge = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_LeftEdgeButtonDisplayText") };
                    _leftEdge.Click += (sender, args) => { _textEditorCore.SetScrollView(_textEditorCore.FlowDirection == FlowDirection.LeftToRight ? ScrollCommand.LeftEdge : ScrollCommand.RightEdge, _scrollBarOrientation); };
                }
                return _leftEdge;
            }
        }

        public MenuFlyoutItem RightEdge
        {
            get
            {
                if (_rightEdge == null)
                {
                    _rightEdge = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_RightEdgeButtonDisplayText") };
                    _rightEdge.Click += (sender, args) => { _textEditorCore.SetScrollView(_textEditorCore.FlowDirection == FlowDirection.LeftToRight ? ScrollCommand.RightEdge : ScrollCommand.LeftEdge, _scrollBarOrientation); };
                }
                return _rightEdge;
            }
        }

        public MenuFlyoutItem PageLeft
        {
            get
            {
                if (_pageLeft == null)
                {
                    _pageLeft = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_PageLeftButtonDisplayText") };
                    _pageLeft.Click += (sender, args) => { _textEditorCore.SetScrollView(_textEditorCore.FlowDirection == FlowDirection.LeftToRight ? ScrollCommand.PageLeft : ScrollCommand.PageRight, _scrollBarOrientation); };
                }
                return _pageLeft;
            }
        }

        public MenuFlyoutItem PageRight
        {
            get
            {
                if (_pageRight == null)
                {
                    _pageRight = new MenuFlyoutItem { Text = _resourceLoader.GetString("TextEditor_ScrollBarContextFlyout_PageRightButtonDisplayText") };
                    _pageRight.Click += (sender, args) => { _textEditorCore.SetScrollView(_textEditorCore.FlowDirection == FlowDirection.LeftToRight ? ScrollCommand.PageRight : ScrollCommand.PageLeft, _scrollBarOrientation); };
                }
                return _pageRight;
            }
        }
    }
}
