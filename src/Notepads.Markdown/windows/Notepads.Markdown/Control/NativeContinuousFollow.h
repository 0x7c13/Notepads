// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>

namespace Notepads::Markdown::Native::FollowCoordinates
{
    struct Line
    {
        uint32_t start{};
        double y{};
        double height{};
    };

    struct Span
    {
        uint32_t start{};
        uint32_t end{};
        double progress{};
    };

    inline bool Valid(Line const &line, uint32_t length) noexcept
    {
        return line.start <= length && std::isfinite(line.y) && std::isfinite(line.height);
    }

    template <typename GetLine> size_t AfterPosition(size_t count, uint32_t position, GetLine const &line)
    {
        size_t first{}, last = count;
        while (first < last)
        {
            const auto middle = first + (last - first) / 2;
            if (line(middle).start <= position)
                first = middle + 1;
            else
                last = middle;
        }
        return first;
    }

    template <typename GetLine> size_t AtPosition(size_t count, uint32_t position, GetLine const &line)
    {
        size_t first{}, last = count;
        while (first < last)
        {
            const auto middle = first + (last - first) / 2;
            if (line(middle).start < position)
                first = middle + 1;
            else
                last = middle;
        }
        return first;
    }

    // No permanent second line table: use the existing Main-ordered, monotone
    // line envelope. The last line at a duplicate start is its canonical vertex.
    template <typename GetLine>
    bool Segment(size_t count, uint32_t length, size_t index, GetLine const &get, Line &first, Line &last)
    {
        first = get(index);
        if (!Valid(first, length))
            return false;
        if (index + 1 < count)
        {
            const auto nextStart = get(index + 1).start;
            last = get(AfterPosition(count, nextStart, get) - 1);
        }
        else
        {
            const double height = length > first.start ? std::max(0.0, first.height) : 0.0;
            last = {length, first.y + height, 0};
        }
        return Valid(last, length) && last.start >= first.start && last.y >= first.y;
    }

    template <typename GetLine>
    bool Ordinate(size_t count, uint32_t length, uint32_t position, GetLine const &get, double &value)
    {
        if (position > length)
            return false;
        if (!count)
        {
            value = 0;
            return true;
        }
        const auto after = AfterPosition(count, position, get);
        const auto index = after ? after - 1 : AfterPosition(count, get(0).start, get) - 1;
        Line first, last;
        if (!Segment(count, length, index, get, first, last))
            return false;
        if (last.start == first.start || last.y == first.y)
            value = first.y;
        else
        {
            const double fraction = std::clamp((static_cast<double>(position) - first.start) /
                                                  (static_cast<double>(last.start) - first.start), 0.0, 1.0);
            value = (1.0 - fraction) * first.y + fraction * last.y;
        }
        return std::isfinite(value);
    }

    template <typename GetLine>
    bool Inverse(size_t count, uint32_t length, double y, GetLine const &get, Span &span)
    {
        if (!std::isfinite(y))
            return false;
        if (!count)
        {
            span = {};
            return true;
        }
        Line tail, terminal;
        if (!Segment(count, length, count - 1, get, tail, terminal))
            return false;
        if (y >= terminal.y)
        {
            span = {length, length, 0};
            return true;
        }
        // upper_bound chooses the increasing-order side of an equal-Y plateau.
        size_t first{}, last = count;
        while (first < last)
        {
            const auto middle = first + (last - first) / 2;
            if (get(middle).y <= y)
                first = middle + 1;
            else
                last = middle;
        }
        auto index = first ? first - 1 : 0;
        index = AfterPosition(count, get(index).start, get) - 1;
        if (get(index).y > y)
        {
            // A duplicate-start group may have a higher canonical Y than its
            // first raw row. The previous group is the real lower vertex.
            const auto begin = AtPosition(count, get(index).start, get);
            if (begin)
                index = begin - 1;
        }
        Line begin, end;
        if (!Segment(count, length, index, get, begin, end))
            return false;
        if (end.start == begin.start || end.y == begin.y)
        {
            span = {end.start, end.start, 0};
            return true;
        }
        span = {begin.start, end.start, std::clamp((y - begin.y) / (end.y - begin.y), 0.0, 1.0)};
        return std::isfinite(span.progress);
    }
}
