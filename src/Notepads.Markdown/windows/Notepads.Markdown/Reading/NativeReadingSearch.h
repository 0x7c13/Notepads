// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <array>
#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string_view>

namespace Notepads::Markdown::Native
{
    inline constexpr uint32_t ReadingSearchMaximumQueryLength = 4096;
    inline constexpr uint32_t ReadingSearchMaximumInFlight = 4;
    inline constexpr uint32_t ReadingSearchWindowCandidates = 8192;

    struct ReadingSearchResult
    {
        bool found{};
        uint32_t start{};
        uint32_t length{};
    };

    // Validates nonempty UTF16 only. The public host API owns its 4096-unit limit.
    bool IsValidReadingSearchQuery(std::wstring_view query) noexcept;

    // Immutable Main search. Does not allocate, copy Main, or inspect live UI state.
    // Empty query and invalid scalar ranges fail; malformed UTF16 query has no match.
    // Checkpoint may throw and is called even when there are no candidates.
    ReadingSearchResult FindReadingText(std::wstring_view text, std::wstring_view query, uint32_t start,
                                       uint32_t length, bool backward, bool ignoreCase,
                                       void (*checkpoint)(void *) = nullptr, void *context = nullptr);

    // The caller's job/context owns this stop state until its body and every queued
    // completion retire. Cancel and Invalidate never retire a gate slot themselves.
    class ReadingSearchOperation final
    {
      public:
        void Cancel() noexcept;
        void Invalidate() noexcept;
        void Check() const;

      private:
        enum class StopReason : uint8_t
        {
            Running,
            Invalidated,
            UserCanceled
        };
        std::atomic<StopReason> _reason{StopReason::Running};
    };

    // Acquire belongs to the UI entry. The mutex also covers final-release
    // invalidation, which may run away from that UI thread. No UI runs under it.
    class ReadingSearchGate final
    {
      public:
        std::shared_ptr<ReadingSearchOperation> Acquire();
        void Invalidate() noexcept;
        uint32_t ActiveCount() const noexcept;

      private:
        mutable std::mutex _mutex;
        std::array<std::weak_ptr<ReadingSearchOperation>, ReadingSearchMaximumInFlight> _slots;
    };
}
