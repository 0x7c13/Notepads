// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownTheme.g.h"
#include <winrt/base.h>
#include <winrt/Windows.UI.h>

namespace winrt::Notepads::Markdown::implementation
{
    // A mutable host input. MarkdownView copies and validates every property at
    // SetTheme; drawing never reads this externally mutable instance.
    struct MarkdownTheme : MarkdownThemeT<MarkdownTheme>
    {
        MarkdownTheme() = default;

        uint64_t Generation() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _generation;
        }
        void Generation(uint64_t value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _generation = value;
        }
        Windows::UI::Color Foreground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _foreground;
        }
        void Foreground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _foreground = value;
        }
        Windows::UI::Color Background() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _background;
        }
        void Background(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _background = value;
        }
        Windows::UI::Color LinkForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _linkForeground;
        }
        void LinkForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _linkForeground = value;
        }
        Windows::UI::Color SelectionForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _selectionForeground;
        }
        void SelectionForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _selectionForeground = value;
        }
        Windows::UI::Color SelectionBackground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _selectionBackground;
        }
        void SelectionBackground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _selectionBackground = value;
        }
        Windows::UI::Color BorderColor() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _borderColor;
        }
        void BorderColor(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _borderColor = value;
        }
        Windows::UI::Color PanelBackground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _panelBackground;
        }
        void PanelBackground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _panelBackground = value;
        }
        Windows::UI::Color FocusColor() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _focusColor;
        }
        void FocusColor(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _focusColor = value;
        }
        Windows::UI::Color CodeCommentForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeCommentForeground;
        }
        void CodeCommentForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeCommentForeground = value;
        }
        Windows::UI::Color CodeStringForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeStringForeground;
        }
        void CodeStringForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeStringForeground = value;
        }
        Windows::UI::Color CodeNumberForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeNumberForeground;
        }
        void CodeNumberForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeNumberForeground = value;
        }
        Windows::UI::Color CodeKeywordForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeKeywordForeground;
        }
        void CodeKeywordForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeKeywordForeground = value;
        }
        Windows::UI::Color CodeTypeForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeTypeForeground;
        }
        void CodeTypeForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeTypeForeground = value;
        }
        Windows::UI::Color CodeFunctionForeground() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeFunctionForeground;
        }
        void CodeFunctionForeground(Windows::UI::Color const &value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeFunctionForeground = value;
        }
        hstring FontFamily() const
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _fontFamily;
        }
        void FontFamily(hstring const &value)
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _fontFamily = value;
        }
        hstring CodeFontFamily() const
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _codeFontFamily;
        }
        void CodeFontFamily(hstring const &value)
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _codeFontFamily = value;
        }
        double FontSize() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _fontSize;
        }
        void FontSize(double value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _fontSize = value;
        }
        double LineHeight() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _lineHeight;
        }
        void LineHeight(double value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _lineHeight = value;
        }
        double Padding() const noexcept
        {
            winrt::slim_shared_lock_guard guard{_propertyLock};
            return _padding;
        }
        void Padding(double value) noexcept
        {
            winrt::slim_lock_guard guard{_propertyLock};
            _padding = value;
        }

      private:
        mutable winrt::slim_mutex _propertyLock;
        uint64_t _generation{1};
        Windows::UI::Color _foreground{255, 30, 30, 30};
        Windows::UI::Color _background{255, 255, 255, 255};
        Windows::UI::Color _linkForeground{255, 0, 90, 158};
        Windows::UI::Color _selectionForeground{255, 255, 255, 255};
        Windows::UI::Color _selectionBackground{255, 0, 90, 158};
        Windows::UI::Color _borderColor{255, 205, 211, 220};
        Windows::UI::Color _panelBackground{255, 244, 245, 247};
        Windows::UI::Color _focusColor{255, 0, 90, 158};
        Windows::UI::Color _codeCommentForeground{255, 90, 112, 81};
        Windows::UI::Color _codeStringForeground{255, 163, 21, 21};
        Windows::UI::Color _codeNumberForeground{255, 9, 134, 88};
        Windows::UI::Color _codeKeywordForeground{255, 0, 0, 180};
        Windows::UI::Color _codeTypeForeground{255, 38, 127, 153};
        Windows::UI::Color _codeFunctionForeground{255, 121, 94, 38};
        hstring _fontFamily{L"Segoe UI"};
        hstring _codeFontFamily{L"Consolas"};
        double _fontSize{18.0};
        double _lineHeight{27.0};
        double _padding{16.0};
    };
}

namespace winrt::Notepads::Markdown::factory_implementation
{
    struct MarkdownTheme : MarkdownThemeT<MarkdownTheme, implementation::MarkdownTheme>
    {
    };
}
