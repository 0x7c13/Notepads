// Scintilla source code edit control
/** @file ScintillaWin.cxx
 ** Windows specific subclass of ScintillaBase.
 **/
 // Copyright 1998-2003 by Neil Hodgson <neilh@scintilla.org>
 // The License.txt file describes the conditions under which this software may be distributed.

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "ScintillaWin.h"
#include <algorithm>
#include <cmath>
#include <limits>

namespace Scintilla::Internal
{
    int64_t ScintillaWinUI::ViewportSourceByte()
    {
        if (_idleSuspended || _idleFinalized || IsViewportRestorePending() ||
            _mainWrapper->Width() <= 0 || _mainWrapper->Height() <= 0 ||
            _mainWrapper->ScrollBarVisibility() != _layoutScrollBars)
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        // The layout owns soft-wrap and bidi row identities. No pixel/DPI
        // conversion, logical-line approximation, or selection mutation is needed.
        RefreshStyleData();
        WrapLines(WrapScope::wsVisible);
        const auto text = GetTextRectangle();
        if (text.Empty() || vs.lineHeight <= 0)
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        const auto currentWrapWidth = Wrapping() ? static_cast<int>(text.Width()) : LineLayout::wrapWidthInfinite;
        if (wrapWidth != currentWrapWidth)
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        const auto [first, end] = VisibleDisplayRows(text, vs.lineHeight);
        if (first >= end || IsViewportRestorePending())
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        const auto line = pcs->DocFromDisplay(first);
        AutoSurface surface(this);
        auto layout = view.RetrieveLineLayout(line, *this);
        if (!surface || !layout) winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        view.LayoutLine(*this, surface, vs, layout.get(), wrapWidth);
        const auto subLine = first - pcs->DisplayFromDoc(line);
        const auto annotationRows = vs.annotationVisible == AnnotationVisible::Hidden ? 0 : pdoc->AnnotationLines(line);
        const auto actualHeight = static_cast<int64_t>(layout->lines) + annotationRows;
        if (actualHeight <= 0 || actualHeight != pcs->GetHeight(line) || subLine < 0 || subLine >= layout->lines)
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        // RangeDisplayLine's same layout offsets, without its unavailable-surface
        // fallback to the logical line start.
        return pdoc->LineStart(line) + layout->SubLineRange(static_cast<int>(subLine), LineLayout::Scope::visibleOnly).start;
    }

    winrt::WinUIEditor::SourceViewportSpan ScintillaWinUI::GetSourceViewport()
    {
        if (_idleSuspended || _idleFinalized || IsViewportRestorePending() || _mainWrapper->Width() <= 0 || _mainWrapper->Height() <= 0 ||
            _mainWrapper->ScrollBarVisibility() != _layoutScrollBars)
            return {};
        const auto document = pdoc;
        const auto revision = _documentRevision;
        const auto length = pdoc->Length();
        RefreshStyleData();
        // Prepare the actual visible display map through the same layout path
        // used by paint. No guessed logical-line fallback is used.
        WrapLines(WrapScope::wsVisible);
        if (IsViewportRestorePending()) return {};
        const auto text = GetTextRectangle();
        if (text.Empty() || vs.lineHeight <= 0 || !std::isfinite(viewportScale) || viewportScale <= 0)
            return {};
        const auto rowHeight = vs.lineHeight;
        const auto scale = viewportScale;
        const auto currentWrapWidth = Wrapping() ? static_cast<int>(text.Width()) : LineLayout::wrapWidthInfinite;
        if (wrapWidth != currentWrapWidth) return {};
        const auto [first, end] = VisibleDisplayRows(text, vs.lineHeight);
        const auto rows = pcs->LinesDisplayed();
        if (first >= end || first < 0 || first >= rows) return {};
        const auto clipped = static_cast<double>(text.top - DisplayRowY(first, vs.lineHeight));
        // VisibleDisplayRows uses the same physical-pixel projection as paint.
        // At an exact row boundary it already names the next row at fraction 0.
        if (!std::isfinite(clipped) || clipped < 0 || clipped >= vs.lineHeight) return {};
        const auto fraction = clipped / vs.lineHeight;
        AutoSurface surface(this);
        if (!surface) return {};
        auto rowStart = [&](Sci::Line row, Sci::Position &position)
        {
            const auto line = pcs->DocFromDisplay(row);
            if (line < 0 || line >= pdoc->LinesTotal()) return false;
            auto layout = view.RetrieveLineLayout(line, *this);
            if (!layout) return false;
            view.LayoutLine(*this, surface, vs, layout.get(), wrapWidth);
            const auto displayStart = pcs->DisplayFromDoc(line);
            const auto subLine = row - displayStart;
            const auto annotationRows = vs.annotationVisible == AnnotationVisible::Hidden ? 0 : pdoc->AnnotationLines(line);
            const auto actualHeight = static_cast<int64_t>(layout->lines) + annotationRows;
            // Visible wrapping is budgeted and can stop before this logical
            // line. Its newly measured layout cannot be combined with old pcs
            // heights, even if both resulting byte positions would be scalars.
            if (actualHeight <= 0 || actualHeight != pcs->GetHeight(line) || subLine < 0 || subLine >= actualHeight)
                return false;
            // An annotation/spacer is not a source text row. Refuse it rather
            // than manufacturing a byte anchor from a neighbouring logical line.
            if (subLine < 0 || subLine >= layout->lines) return false;
            position = pdoc->LineStart(line) +
                layout->SubLineRange(static_cast<int>(subLine), LineLayout::Scope::visibleOnly).start;
            return position >= 0 && position <= length &&
                pdoc->MovePositionOutsideChar(position, -1, false) == position;
        };
        Sci::Position start{};
        Sci::Position next = length;
        if (!rowStart(first, start) || (first < rows - 1 && !rowStart(first + 1, next)) || next < start)
            return {};
        if (pdoc != document || _documentRevision != revision || pdoc->Length() != length ||
            IsViewportRestorePending() || _mainWrapper->ScrollBarVisibility() != _layoutScrollBars ||
            vs.lineHeight != rowHeight || viewportScale != scale || wrapWidth != currentWrapWidth ||
            GetTextRectangle().Width() != text.Width() || pcs->LinesDisplayed() != rows ||
            VisibleDisplayRows(GetTextRectangle(), vs.lineHeight).first != first ||
            static_cast<double>(GetTextRectangle().top - DisplayRowY(first, vs.lineHeight)) != clipped)
            return {};
        return {start, next, first, fraction, true};
    }

    double ScintillaWinUI::VerticalScrollExtent() const
    {
        if (_idleSuspended || _idleFinalized || IsViewportRestorePending() || _mainWrapper->Height() <= 0 ||
            _mainWrapper->ScrollBarVisibility() != _layoutScrollBars)
            return -1;
        const auto geometry = ScrollGeometry();
        return geometry.Distance({}, geometry.Maximum());
    }

    double ScintillaWinUI::VerticalScrollProgress() const
    {
        const auto maximum = VerticalScrollExtent();
        if (maximum <= 0)
            return -1;
        const auto geometry = ScrollGeometry();
        const auto end = geometry.Maximum();
        const VerticalScrollPosition position{topLine, verticalOffset};
        const auto rowsToEnd = end.row - position.row;
        const int endPixel = geometry.PixelOffset(end);
        // A fractional thumb position can paint at the same pixel as the exact end.
        if (rowsToEnd >= 0 && rowsToEnd <= 1 && (end.row > 0 || endPixel > 0) &&
            geometry.PixelOffset(position) == rowsToEnd * vs.lineHeight + endPixel)
            return 1;
        return std::clamp(geometry.Distance({}, position) / maximum, 0.0, 1.0);
    }

    bool ScintillaWinUI::TrySetVerticalScrollProgress(double progress)
    {
        if (!std::isfinite(progress) || progress < 0 || progress > 1)
            throw winrt::hresult_invalid_argument();
        if (_idleSuspended || _idleFinalized || IsViewportRestorePending() || _mainWrapper->Height() <= 0 ||
            _mainWrapper->ScrollBarVisibility() != _layoutScrollBars)
            return false;
        const auto attachment = _documentAttachmentGeneration;
        const auto revision = _documentRevision;
        const auto selection = selectionRevision;
        const auto intent = _scrollAnimationRevision + 1;
        auto current = [&]()
        {
            return !_idleSuspended && !_idleFinalized && _documentAttachmentGeneration == attachment &&
                _documentRevision == revision && selectionRevision == selection && _scrollAnimationRevision == intent;
        };
        CancelSmoothScroll();
        if (!current())
            return false;
        const auto requested = [&](VerticalScrollGeometry const &geometry)
        {
            return progress == 1 ? geometry.Maximum() :
                geometry.Add({}, progress * geometry.Distance({}, geometry.Maximum()));
        };
        if (requested(ScrollGeometry()) == VerticalScrollPosition{topLine, verticalOffset})
            return true;
        if (!BeginScrollInteraction(false, true) || !current())
            return false;
        RefreshStyleData();
        if (!current())
            return false;
        ScrollToPosition(requested(ScrollGeometry()));
        if (!current())
            return false;
        NotifyUpdateUI();
        return current();
    }

    bool ScintillaWinUI::SetViewportBetweenSourcePositions(int64_t start, int64_t end, double progress)
    {
        if (!std::isfinite(progress) || progress < 0 || progress > 1 || start < 0 || end < start)
            throw winrt::hresult_invalid_argument();
        if (_idleSuspended || _idleFinalized)
            return false;
        const auto document = pdoc;
        const auto attachment = _documentAttachmentGeneration;
        const auto revision = _documentRevision;
        const auto selection = selectionRevision;
        const auto length = pdoc->Length();
        if (end > length || pdoc->MovePositionOutsideChar(static_cast<Sci::Position>(start), -1, false) != start ||
            pdoc->MovePositionOutsideChar(static_cast<Sci::Position>(end), -1, false) != end)
            throw winrt::hresult_invalid_argument();
        // Retirement precedes every external observer. The expected revision is
        // derived before cancellation, so a reentrant cancel is not adopted.
        const auto intent = _scrollAnimationRevision + 1;
        auto current = [&]()
        {
            return !_idleSuspended && !_idleFinalized && pdoc == document &&
                _documentAttachmentGeneration == attachment && _documentRevision == revision &&
                selectionRevision == selection && pdoc->Length() == length && _scrollAnimationRevision == intent;
        };
        CancelSmoothScroll();
        if (!current() || !BeginScrollInteraction(false, true) || !current())
            return false;
        RefreshStyleData();
        if (!current())
            return false;
        WrapLines(WrapScope::wsVisible);
        if (!current())
            return false;
        const auto text = GetTextRectangle();
        const auto scale = viewportScale;
        const auto lineHeight = vs.lineHeight;
        const bool wrapping = Wrapping();
        const auto width = wrapping ? static_cast<int>(text.Width()) : LineLayout::wrapWidthInfinite;
        if (text.Empty() || lineHeight <= 0 || !std::isfinite(scale) || scale <= 0 || wrapWidth != width ||
            _mainWrapper->Width() <= 0 || _mainWrapper->Height() <= 0 ||
            _mainWrapper->ScrollBarVisibility() != _layoutScrollBars)
            return false;
        auto geometryCurrent = [&]()
        {
            if (!current() || !stylesValid)
                return false;
            const auto bounds = GetTextRectangle();
            return Wrapping() == wrapping && wrapWidth == width && vs.lineHeight == lineHeight &&
                viewportScale == scale && bounds.Width() == text.Width() && bounds.Height() == text.Height() &&
                _mainWrapper->Width() > 0 && _mainWrapper->Height() > 0 &&
                _mainWrapper->ScrollBarVisibility() == _layoutScrollBars && current();
        };
        AutoSurface surface(this);
        if (!surface || !geometryCurrent())
            return false;
        auto prepare = [&](Sci::Position position)
        {
            const auto line = pdoc->SciLineFromPosition(position);
            if (!pcs->GetVisible(line))
                return false;
            pdoc->EnsureStyledTo(pdoc->LineStart(line + 1));
            if (!geometryCurrent())
                return false;
            if (wrapping)
                WrapOneLine(surface, line);
            return geometryCurrent();
        };
        if (!prepare(static_cast<Sci::Position>(start)) || !prepare(static_cast<Sci::Position>(end)))
            return false;
        // Preparing the second logical line may change preceding display-row
        // heights. Resolve both points only after both layouts are prepared.
        const auto rows = pcs->LinesDisplayed();
        struct Point
        {
            Sci::Line row{};
            double fraction{};
        };
        auto point = [&](Sci::Position position, Point &result)
        {
            if (!geometryCurrent())
                return false;
            const auto line = pdoc->SciLineFromPosition(position);
            if (!pcs->GetVisible(line))
                return false;
            auto layout = view.RetrieveLineLayout(line, *this);
            if (!layout)
                return false;
            view.LayoutLine(*this, surface, vs, layout.get(), wrapWidth);
            if (!geometryCurrent())
                return false;
            const auto annotationRows = vs.annotationVisible == AnnotationVisible::Hidden ? 0 : pdoc->AnnotationLines(line);
            if (layout->lines <= 0 || static_cast<int64_t>(layout->lines) + annotationRows != pcs->GetHeight(line))
                return false;
            const auto row = view.DisplayFromPosition(surface, *this, SelectionPosition(position), vs);
            if (!geometryCurrent() || rows != pcs->LinesDisplayed() || row < 0 || row >= rows)
                return false;
            const auto subLine = row - pcs->DisplayFromDoc(line);
            if (subLine < 0 || subLine >= layout->lines)
                return false;
            const auto lineStart = pdoc->LineStart(line);
            const auto first = lineStart + layout->SubLineRange(static_cast<int>(subLine), LineLayout::Scope::visibleOnly).start;
            const auto next = subLine + 1 < layout->lines ?
                lineStart + layout->SubLineRange(static_cast<int>(subLine + 1), LineLayout::Scope::visibleOnly).start :
                pdoc->LineStart(line + 1);
            if (first < 0 || next < first || next > length || position < first || position > next)
                return false;
            result = {row, next > first ? static_cast<double>(position - first) / static_cast<double>(next - first) : 0};
            return true;
        };
        Point first, last;
        if (!point(static_cast<Sci::Position>(start), first) || !point(static_cast<Sci::Position>(end), last) ||
            !geometryCurrent() || rows != pcs->LinesDisplayed())
            return false;
        const double distance = static_cast<double>(last.row - first.row) + last.fraction - first.fraction;
        const double local = first.fraction + progress * distance;
        const double whole = std::floor(local);
        if (distance < 0 || !std::isfinite(local) || whole < 0 ||
            whole > static_cast<double>(std::numeric_limits<Sci::Line>::max() - first.row))
            return false;
        const auto row = first.row + static_cast<Sci::Line>(whole);
        const double offset = (local - whole) * (static_cast<double>(lineHeight) / scale);
        // Begin was already accepted and checked. A public SetVerticalViewport
        // call here would notify observers again and could silently reject.
        const auto target = ScrollGeometry().Clamp({row, offset});
        ScrollToPosition(target);
        auto admitted = [&]()
        {
            return geometryCurrent() && rows == pcs->LinesDisplayed() &&
                topLine == target.row && verticalOffset == target.offset;
        };
        if (!admitted())
            return false;
        NotifyUpdateUI();
        const bool accepted = admitted();
        if (accepted) _textPrefetchRequested = true;
        return accepted;
    }

    void ScintillaWinUI::RevealSourceByte(int64_t byteOffset)
    {
        if (byteOffset < 0 || byteOffset > pdoc->Length() ||
            _mainWrapper->Width() <= 0 || _mainWrapper->Height() <= 0)
            throw winrt::hresult_invalid_argument();
        const auto position = static_cast<Sci::Position>(byteOffset);
        if (pdoc->MovePositionOutsideChar(position, -1, false) != position)
            throw winrt::hresult_invalid_argument();
        RefreshStyleData();
        if (GetTextRectangle().Empty() || vs.lineHeight <= 0)
            winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        const auto line = pdoc->SciLineFromPosition(position);
        pdoc->EnsureStyledTo(pdoc->LineStart(line + 1));
        AutoSurface surface(this);
        auto layout = view.RetrieveLineLayout(line, *this);
        if (!surface || !layout) winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
        if (Wrapping())
        {
            // Prepare this anchor even when lazy wrapping has not reached it.
            // RestoreViewport will prepare/clamp its final viewport before paint.
            wrapWidth = static_cast<int>(GetTextRectangle().Width());
            WrapOneLine(surface, line);
        }
        else
        {
            WrapLines(WrapScope::wsVisible);
        }
        const auto row = view.DisplayFromPosition(surface, *this, SelectionPosition(position), vs);
        const auto subLine = row - pcs->DisplayFromDoc(line);
        RestoreViewport(DisplayHorizontalOffset(), line, std::max<Sci::Line>(0, subLine), 0, true);
    }

}
