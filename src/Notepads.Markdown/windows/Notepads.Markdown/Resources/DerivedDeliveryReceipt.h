// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <cstdint>
#include <utility>

namespace Notepads::Markdown::Native
{
    // One prepaid transport slot owns this receipt and its intrusive owner.
    // Registration may invoke synchronously; settlement is independent of the
    // worker's return and never changes a newer source attempt.
    class DerivedDeliveryReceipt final
    {
      public:
        using Settle = void (*)(void *, uint64_t, uint32_t, bool) noexcept;
        using Release = void (*)(void *) noexcept;
        DerivedDeliveryReceipt() noexcept = default;
        DerivedDeliveryReceipt(void *owner, uint64_t attempt, uint32_t ordinal, Settle settle, Release release) noexcept
            : _owner(owner), _attempt(attempt), _ordinal(ordinal), _settle(settle), _release(release) {}
        DerivedDeliveryReceipt(DerivedDeliveryReceipt const &) = delete;
        DerivedDeliveryReceipt &operator=(DerivedDeliveryReceipt const &) = delete;
        DerivedDeliveryReceipt(DerivedDeliveryReceipt &&other) noexcept
            : _owner(std::exchange(other._owner, nullptr)), _attempt(other._attempt), _ordinal(other._ordinal),
              _settle(other._settle), _release(other._release) {}
        ~DerivedDeliveryReceipt() { Complete(false); }
        void Complete(bool accepted) noexcept
        {
            if (auto owner = std::exchange(_owner, nullptr))
            {
                _settle(owner, _attempt, _ordinal, accepted);
                _release(owner);
            }
        }

      private:
        void *_owner{};
        uint64_t _attempt{};
        uint32_t _ordinal{};
        Settle _settle{};
        Release _release{};
    };
}
