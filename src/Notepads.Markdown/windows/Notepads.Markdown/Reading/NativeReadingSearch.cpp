// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Reading/NativeReadingSearch.h"
#include <algorithm>
#include <limits>

namespace Notepads::Markdown::Native
{
    namespace
    {
        bool HighSurrogate(wchar_t value) noexcept
        {
            return value >= 0xD800 && value <= 0xDBFF;
        }

        bool LowSurrogate(wchar_t value) noexcept
        {
            return value >= 0xDC00 && value <= 0xDFFF;
        }

        bool Boundary(std::wstring_view text, uint32_t position) noexcept
        {
            return position <= text.size() &&
                   (position == 0 || position == text.size() || !HighSurrogate(text[position - 1]) ||
                    !LowSurrogate(text[position]));
        }

        void Checkpoint(void (*checkpoint)(void *), void *context)
        {
            if (checkpoint)
                checkpoint(context);
        }
    }

    bool IsValidReadingSearchQuery(std::wstring_view query) noexcept
    {
        if (query.empty())
            return false;
        for (size_t index = 0; index < query.size(); ++index)
        {
            if (HighSurrogate(query[index]))
            {
                if (index + 1 == query.size() || !LowSurrogate(query[index + 1]))
                    return false;
                ++index;
            }
            else if (LowSurrogate(query[index]))
            {
                return false;
            }
        }
        return true;
    }

    ReadingSearchResult FindReadingText(std::wstring_view text, std::wstring_view query, uint32_t start,
                                       uint32_t length, bool backward, bool ignoreCase,
                                       void (*checkpoint)(void *), void *context)
    {
        Checkpoint(checkpoint, context);
        // Addition is checked in 64 bits before a result offset can be narrowed.
        auto end64 = static_cast<uint64_t>(start) + length;
        if (end64 > std::numeric_limits<uint32_t>::max() || start > text.size() || length > text.size() - start)
            throw winrt::hresult_invalid_argument();
        auto end = static_cast<uint32_t>(end64);
        if (!Boundary(text, start) || !Boundary(text, end) || query.empty())
            throw winrt::hresult_invalid_argument();
        if (query.size() > length || !IsValidReadingSearchQuery(query))
        {
            Checkpoint(checkpoint, context);
            return {};
        }

        constexpr auto intMaximum = static_cast<uint32_t>(std::numeric_limits<int>::max());
        if (query.size() > intMaximum)
            throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW));
        auto count = static_cast<uint32_t>(query.size());
        // The overlap is query.length - 1, so a window has at most this many
        // candidate starts while both explicit NLS lengths remain representable.
        auto windowCandidates = std::min(ReadingSearchWindowCandidates, intMaximum - count + 1);
        auto candidateEnd = end - count + 1; // exclusive; count is nonzero
        auto remainingFirst = start;
        auto remainingEnd = candidateEnd;
        while (remainingFirst < remainingEnd)
        {
            auto candidates = std::min(windowCandidates, remainingEnd - remainingFirst);
            auto first = backward ? remainingEnd - candidates : remainingFirst;
            auto limit = first + candidates;
            auto searchFirst = first;
            auto searchEnd = limit;
            while (searchFirst < searchEnd)
            {
                auto windowLength = searchEnd - searchFirst + count - 1;
                Checkpoint(checkpoint, context);
                // -1 means both NotFound and API failure. Clear/read last error
                // immediately around NLS; a checkpoint may itself call Win32.
                ::SetLastError(ERROR_SUCCESS);
                auto relative = ::FindStringOrdinal(backward ? FIND_FROMEND : FIND_FROMSTART,
                                                    text.data() + searchFirst, static_cast<int>(windowLength),
                                                    query.data(), static_cast<int>(count), ignoreCase ? TRUE : FALSE);
                auto error = ::GetLastError();
                Checkpoint(checkpoint, context);
                if (relative == -1)
                {
                    if (error != ERROR_SUCCESS)
                        throw winrt::hresult_error(HRESULT_FROM_WIN32(error));
                    break;
                }
                if (relative < 0 || static_cast<uint32_t>(relative) >= searchEnd - searchFirst)
                    throw winrt::hresult_error(E_UNEXPECTED);
                auto position = searchFirst + static_cast<uint32_t>(relative);
                if (Boundary(text, position) && Boundary(text, position + count))
                {
                    Checkpoint(checkpoint, context);
                    return {true, position, count};
                }
                // A rejected scalar endpoint must not hide a later overlapping
                // candidate in this window, including backward matches.
                if (backward)
                    searchEnd = position;
                else
                    searchFirst = position + 1;
            }
            if (backward)
                remainingEnd = first;
            else
                remainingFirst = limit;
        }
        Checkpoint(checkpoint, context);
        return {};
    }

    void ReadingSearchOperation::Cancel() noexcept
    {
        _reason.store(StopReason::UserCanceled, std::memory_order_release);
    }

    void ReadingSearchOperation::Invalidate() noexcept
    {
        auto expected = StopReason::Running;
        (void)_reason.compare_exchange_strong(expected, StopReason::Invalidated, std::memory_order_acq_rel,
                                             std::memory_order_acquire);
    }

    void ReadingSearchOperation::Check() const
    {
        switch (_reason.load(std::memory_order_acquire))
        {
        case StopReason::UserCanceled:
            throw winrt::hresult_canceled();
        case StopReason::Invalidated:
            throw winrt::hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        case StopReason::Running:
            return;
        }
    }

    std::shared_ptr<ReadingSearchOperation> ReadingSearchGate::Acquire()
    {
        std::lock_guard lock{_mutex};
        for (auto &slot : _slots)
        {
            if (!slot.expired())
                continue;
            auto operation = std::make_shared<ReadingSearchOperation>();
            slot = operation;
            return operation;
        }
        throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_BUSY));
    }

    void ReadingSearchGate::Invalidate() noexcept
    {
        std::lock_guard lock{_mutex};
        for (auto const &slot : _slots)
            if (auto operation = slot.lock())
                operation->Invalidate();
    }

    uint32_t ReadingSearchGate::ActiveCount() const noexcept
    {
        std::lock_guard lock{_mutex};
        uint32_t count{};
        for (auto const &slot : _slots)
            if (!slot.expired())
                ++count;
        return count;
    }
}
