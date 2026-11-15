// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Images/NativeImagePresentation.h"
#include <algorithm>
#include <cstring>

namespace Notepads::Markdown::Native
{
    // Per-control completed raster payloads only. Every hit follows a newly
    // authorized provider Open/read/Close. No request/model/source survives here.
    class NativeImageReuseCache final
    {
        struct ConstructionKey
        {
        };
        static constexpr uint32_t RasterRevision = 1;

      public:
        static constexpr size_t SlotCount = 32;
        static constexpr size_t CompareChunkBytes = 64 * 1024;
        struct Candidate
        {
            // Builder type is retained solely to avoid casting away constness.
            // Insert/lookup both require frozen storage; no write is performed.
            std::shared_ptr<ImagePixels> pixels;
            NativeEncodedImageHandle encoded;
            explicit operator bool() const noexcept
            {
                return pixels && encoded;
            }
        };
        struct Resident
        {
            // Identity values are compared only, never dereferenced. This
            // non-owning accounting snapshot cannot pin 32 payloads across IO.
            ImagePixels const *pixels{};
            NativeEncodedImage const *encoded{};
            uint64_t pixelCapacity{}, encodedCapacity{}, lastUse{}, entry{};
            size_t slot{};
        };
        struct Residents
        {
            std::array<Resident, SlotCount> entries{};
            uint64_t scope{}, metadataRequestedBytes{};
            size_t count{};
        };

        explicit NativeImageReuseCache(ConstructionKey) noexcept : _scope(NextScope())
        {
        }
        static std::shared_ptr<NativeImageReuseCache> Create(std::shared_ptr<ReadingMemory> const &memory)
        {
            if (!memory)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            Allocation allocation;
            ImagePayloadConstruction construction{*memory, {&allocation, Allocate}};
            auto result = std::allocate_shared<NativeImageReuseCache>(
                ImagePayloadAllocator<NativeImageReuseCache>{construction.Nonce()}, ConstructionKey{});
            result->_metadataRequestedBytes = ReadingMemory::AllocationRequestBytes(allocation.bytes);
            return result;
        }
        NativeImageReuseCache(NativeImageReuseCache const &) = delete;
        NativeImageReuseCache &operator=(NativeImageReuseCache const &) = delete;

        uint64_t Scope() const noexcept
        {
            std::lock_guard guard(_mutex);
            return _scope;
        }
        // Provider/unload/pressure invalidation also revokes late insertion.
        // Final payload/prefix release takes place after the cache lock ends.
        void Clear() noexcept
        {
            std::array<Slot, SlotCount> retired;
            {
                std::lock_guard guard(_mutex);
                _scope = NextScope();
                for (size_t index = 0; index < SlotCount; ++index)
                    retired[index] = std::move(_slots[index]);
            }
        }
        Residents Inspect() const noexcept
        {
            Residents result;
            std::lock_guard guard(_mutex);
            result.scope = _scope;
            result.metadataRequestedBytes = _metadataRequestedBytes;
            for (size_t index = 0; index < SlotCount; ++index)
            {
                auto const &slot = _slots[index];
                if (!slot.value)
                    continue;
                result.entries[result.count++] = {slot.value.pixels.get(), slot.value.encoded.Get(),
                    slot.value.pixels->pixels.capacity(), slot.value.encoded->Capacity(), slot.lastUse, slot.entry, index};
            }
            return result;
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // A real owner clone under the normal lock. Tests never reconstruct a
        // handle from Inspect's value-only identities or bypass last-owner free.
        NativeEncodedImageHandle CloneEncodedForDiagnostics() const
        {
            std::lock_guard guard(_mutex);
            for (auto const &slot : _slots)
                if (slot.value) return slot.value.encoded;
            return {};
        }
#endif
        // UI pressure first chooses among these cache identities and current
        // cold rows using one LRU/128-MiB target. Erasing is not byte-capacity return.
        bool Remove(uint64_t scope, size_t index, uint64_t entry) noexcept
        {
            Slot retired;
            {
                std::lock_guard guard(_mutex);
                if (!scope || scope != _scope || index >= SlotCount || _slots[index].entry != entry)
                    return false;
                retired = std::move(_slots[index]);
                _slots[index] = {};
            }
            return true;
        }
        // Caller has already successfully committed a current publication. The
        // input is moved by value before locking, so no AddRef allocation/fault
        // or original last-owner destruction occurs in the critical section.
        bool Insert(uint64_t scope, Candidate value, uint64_t lastUse) noexcept
        {
            if (!IsRaster(value))
                return false;
            std::array<Slot, SlotCount> retired;
            size_t retiredCount{};
            bool kept{};
            {
                std::lock_guard guard(_mutex);
                if (!scope || scope != _scope || _nextEntry == UINT64_MAX ||
                    _metadataRequestedBytes >= ImageOffscreenWarmTargetBytes ||
                    value.encoded->Capacity() > ImageOffscreenWarmTargetBytes - _metadataRequestedBytes)
                    return false;
                size_t selected = SlotCount;
                for (size_t index = 0; index < SlotCount; ++index)
                    if (_slots[index].value.pixels == value.pixels)
                    {
                        _slots[index].lastUse = lastUse;
                        return true;
                    }
                    else if (_slots[index].value && _slots[index].value.encoded.Get() == value.encoded.Get())
                        return false; // One exact encoded owner per slot/plan.
                    else if (!_slots[index].value && selected == SlotCount)
                        selected = index;
                if (selected == SlotCount)
                {
                    selected = 0;
                    for (size_t index = 1; index < SlotCount; ++index)
                        if (_slots[index].lastUse < _slots[selected].lastUse)
                            selected = index;
                }
                if (_slots[selected].value)
                    retired[retiredCount++] = std::move(_slots[selected]);
                _slots[selected] = {std::move(value), ++_nextEntry, lastUse, RasterRevision};
                auto inserted = _nextEntry;
                // This is a subset of the one shared warm target, not another
                // allowance. UI also counts cold decoded/current B2 owners.
                // Keeping this invariant under the same mutex bounds lookup's
                // full comparison work before the stricter UI union trim.
                uint64_t encodedBytes = _metadataRequestedBytes;
                for (auto const &slot : _slots)
                    if (slot.value)
                        encodedBytes += slot.value.encoded->Capacity();
                while (encodedBytes > ImageOffscreenWarmTargetBytes)
                {
                    size_t oldest = SlotCount;
                    for (size_t index = 0; index < SlotCount; ++index)
                        if (_slots[index].value && (oldest == SlotCount || _slots[index].lastUse < _slots[oldest].lastUse))
                            oldest = index;
                    if (oldest == SlotCount || retiredCount == SlotCount)
                        std::terminate(); // Fixed batch proof/invariant failure.
                    encodedBytes -= _slots[oldest].value.encoded->Capacity();
                    retired[retiredCount++] = std::move(_slots[oldest]);
                    _slots[oldest] = {};
                }
                kept = _slots[selected].value && _slots[selected].entry == inserted;
            }
            return kept;
        }
        // No hash authority or unconditional content pass. Each encoded owner
        // appears once, and their total capacities are bounded by the shared
        // warm target; exact comparison work is therefore bounded likewise.
        template <typename Checkpoint>
        Candidate FindRaster(uint64_t scope, std::span<uint8_t const> freshBytes,
                             NativeImageRequest const &request, Checkpoint const &checkpoint) const
        {
            if (!scope || request.Kind() == NOTEPADS_MARKDOWN_IMAGE_SVG)
                return {};
            auto active = [&]
            {
                if (!request.IsCurrent())
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED};
                checkpoint();
            };
            active();
            uint64_t entryBoundary{};
            {
                std::lock_guard guard(_mutex);
                if (scope != _scope)
                    return {};
                entryBoundary = _nextEntry;
            }
            uint64_t comparedBytes{};
            for (size_t index = 0; index < SlotCount; ++index)
            {
                Candidate candidate;
                {
                    std::lock_guard guard(_mutex);
                    if (scope != _scope)
                        return {};
                    auto const &slot = _slots[index];
                    if (!slot.value || slot.entry > entryBoundary || slot.revision != RasterRevision ||
                        slot.value.encoded->Bytes().size() != freshBytes.size())
                        continue;
                    // NativeEncodedImage AddRef can reject only saturation.
                    // No external callback or final release is performed here.
                    try
                    {
                        candidate.encoded = slot.value.encoded;
                    }
                    catch (...)
                    {
                        continue; // Optional saturated candidate falls back.
                    }
                    candidate.pixels = slot.value.pixels;
                }
                active();
                if (!ExactRasterPlan(candidate, request.Target()))
                    continue;
                auto prior = candidate.encoded->Bytes();
                bool equal = true;
                for (size_t offset = 0; offset < freshBytes.size();)
                {
                    active();
                    auto count = (std::min)(CompareChunkBytes, freshBytes.size() - offset);
                    if (count > ImageOffscreenWarmTargetBytes - comparedBytes)
                        return {}; // Bounded ordinary miss, independent of aliases.
                    comparedBytes += count;
                    if (std::memcmp(prior.data() + offset, freshBytes.data() + offset, count) != 0)
                    {
                        equal = false;
                        break;
                    }
                    offset += count;
                }
                active();
                if (equal)
                {
                    std::lock_guard guard(_mutex);
                    if (scope == _scope)
                        return candidate;
                    return {};
                }
            }
            return {};
        }

      private:
        static uint64_t NextScope() noexcept
        {
            static std::atomic<uint64_t> sequence{};
            auto previous = sequence.load(std::memory_order_relaxed);
            while (previous != UINT64_MAX)
                if (sequence.compare_exchange_weak(previous, previous + 1, std::memory_order_relaxed))
                    return previous + 1;
            return 0; // Module exhaustion permanently disables new scopes.
        }
        struct Allocation
        {
            size_t bytes{};
        };
        static void *Allocate(void *context, ReadingMemory const &memory, size_t bytes)
        {
            auto &allocation = *static_cast<Allocation *>(context);
            if (allocation.bytes)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            allocation.bytes = bytes;
            return memory.Allocate(bytes);
        }
        struct Slot
        {
            Candidate value;
            uint64_t entry{}, lastUse{};
            uint32_t revision{};
        };
        static bool IsRaster(Candidate const &value) noexcept
        {
            return value && value.pixels->IsFrozen() && !value.pixels->IsAnimated() && value.encoded->IsFrozen() &&
                value.pixels->metadata.kind == NOTEPADS_MARKDOWN_IMAGE_RASTER &&
                ValidImageMetadata(value.pixels->metadata, value.pixels->pixels.size()) &&
                value.pixels->charge && value.pixels->charge->IsAdmitted(NOTEPADS_MARKDOWN_IMAGE_DECODED, value.pixels->pixels.capacity()) &&
                !value.encoded->Bytes().empty() && value.encoded.Charge() &&
                value.encoded.Charge()->IsAdmitted(NOTEPADS_MARKDOWN_IMAGE_ENCODED, value.encoded->Capacity());
        }
        static bool ExactRasterPlan(Candidate const &value, notepads_markdown_image_target const &target) noexcept
        {
            if (!IsRaster(value))
                return false;
            auto plan = notepads_markdown_image_raster_plan{
                {NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION, sizeof(notepads_markdown_image_raster_plan)}};
            auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
            auto const &metadata = value.pixels->metadata;
            return notepads_markdown_image_plan_raster(&metadata, &target, &plan, &error) == NOTEPADS_MARKDOWN_CORE_OK &&
                plan.width == metadata.decoded_width && plan.height == metadata.decoded_height &&
                plan.stride == metadata.stride && plan.bytes == value.pixels->pixels.capacity();
        }
        mutable std::mutex _mutex;
        std::array<Slot, SlotCount> _slots;
        uint64_t _scope{}, _nextEntry{}, _metadataRequestedBytes{};
    };
}
