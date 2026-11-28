// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Rendering/NativeMeasurementCache.h"
#include <bit>
#include <cmath>
#include <limits>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_METRIC_CACHE_VERSION, sizeof(T)};
            return value;
        }
        constexpr size_t NativeLimit = 1024 * 1024;
        struct alignas(MEMORY_ALLOCATION_ALIGNMENT) AllocationPrefix
        {
            ModuleLifetime module;
            std::array<uint8_t, MEMORY_ALLOCATION_ALIGNMENT - sizeof(ModuleLifetime)> padding{};
        };
        template <typename T> struct CacheAllocator
        {
            using value_type = T;
            // The observer is dereferenced only by allocate_shared's synchronous
            // allocation. Copies retained in its control block never dereference
            // it during destruction or comparison.
            size_t *observed{};
            explicit CacheAllocator(size_t *value) noexcept : observed(value)
            {
            }
            template <typename U> CacheAllocator(CacheAllocator<U> const &other) noexcept : observed(other.observed)
            {
            }
            T *allocate(size_t count)
            {
                static_assert(alignof(T) <= MEMORY_ALLOCATION_ALIGNMENT);
                if (count > (NativeLimit - sizeof(AllocationPrefix)) / sizeof(T) || *observed != 0)
                    throw std::bad_alloc();
                auto requested = count * sizeof(T) + sizeof(AllocationPrefix);
                auto raw = HeapAlloc(GetProcessHeap(), 0, requested);
                if (!raw)
                    throw std::bad_alloc();
                auto actual = HeapSize(GetProcessHeap(), 0, raw);
                if (actual == SIZE_T(-1) || actual < requested || actual > NativeLimit)
                {
                    HeapFree(GetProcessHeap(), 0, raw);
                    throw std::bad_alloc();
                }
                auto prefix = new (raw) AllocationPrefix{};
                *observed = actual;
                return reinterpret_cast<T *>(prefix + 1);
            }
            void deallocate(T *memory, size_t) noexcept
            {
                ModuleLifetime pin;
                auto prefix = reinterpret_cast<AllocationPrefix *>(memory) - 1;
                prefix->~AllocationPrefix();
                HeapFree(GetProcessHeap(), 0, prefix);
            }
            template <typename U> bool operator==(CacheAllocator<U> const &other) const noexcept
            {
                return observed == other.observed;
            }
        };
    }

    uint32_t TextMetricView::Word(size_t offset) const noexcept
    {
        if (offset > _bytes.size() || _bytes.size() - offset < 4)
            return 0;
        return uint32_t(_bytes[offset]) | (uint32_t(_bytes[offset + 1]) << 8) | (uint32_t(_bytes[offset + 2]) << 16) |
               (uint32_t(_bytes[offset + 3]) << 24);
    }
    uint32_t TextMetricView::Length() const noexcept
    {
        return Word(12);
    }
    uint32_t TextMetricView::ClusterCount() const noexcept
    {
        return Word(16);
    }
    uint32_t TextMetricView::LineCount() const noexcept
    {
        return Word(20);
    }
    float TextMetricView::Height() const noexcept
    {
        return std::bit_cast<float>(Word(24));
    }
    float TextMetricView::Width() const noexcept
    {
        return std::bit_cast<float>(Word(28));
    }
    float TextMetricView::OverflowAbove() const noexcept
    {
        return std::bit_cast<float>(Word(32));
    }
    float TextMetricView::OverflowBelow() const noexcept
    {
        return std::bit_cast<float>(Word(36));
    }
    uint32_t TextMetricView::Cluster(uint32_t index) const noexcept
    {
        return index < ClusterCount() ? Word(HeaderBytes + size_t(index) * 4) : 0;
    }
    TextMetricLine TextMetricView::Line(uint32_t index) const noexcept
    {
        if (index >= LineCount())
            return {};
        auto offset = HeaderBytes + size_t(ClusterCount()) * 4 + size_t(index) * 12;
        return {Word(offset), Word(offset + 4), std::bit_cast<float>(Word(offset + 8))};
    }
    bool TextMetricView::Valid(uint32_t expectedLength) const noexcept
    {
        if (_bytes.size() < HeaderBytes || _bytes.size() > NOTEPADS_MARKDOWN_METRIC_CACHE_ENTRY_BYTES_LIMIT || Word(0) != Magic ||
            Word(4) != 2 || Word(8) != _bytes.size() || Length() != expectedLength ||
            uint64_t(HeaderBytes) + uint64_t(ClusterCount()) * 4 + uint64_t(LineCount()) * 12 != _bytes.size() ||
            !LineCount() || !std::isfinite(Height()) || Height() <= 0 || !std::isfinite(Width()) || Width() <= 0 ||
            !std::isfinite(OverflowAbove()) || OverflowAbove() < 0 ||
            !std::isfinite(OverflowBelow()) || OverflowBelow() < 0)
            return false;
        uint64_t total{};
        for (uint32_t i = 0; i < ClusterCount(); ++i)
        {
            if (!Cluster(i))
                return false;
            total += Cluster(i);
        }
        if (total != expectedLength)
            return false;
        total = 0;
        for (uint32_t i = 0; i < LineCount(); ++i)
        {
            auto line = Line(i);
            if (line.newlineLength > line.length || !std::isfinite(line.height) || line.height <= 0)
                return false;
            total += line.length;
        }
        return total == expectedLength;
    }

    NativeMeasurementCache::NativeMeasurementCache(ConstructionKey) noexcept
    {
        notepads_markdown_metric_cache_header request{NOTEPADS_MARKDOWN_METRIC_CACHE_VERSION, sizeof(notepads_markdown_metric_cache_header)};
        auto stats = Pod<notepads_markdown_metric_cache_stats>();
        if (notepads_markdown_metric_cache_create(&request, &_cache, &stats) != 0)
        {
            notepads_markdown_metric_cache_destroy(_cache);
            _cache = nullptr;
        }
    }
    NativeMeasurementCache::~NativeMeasurementCache()
    {
        notepads_markdown_metric_cache_destroy(_cache);
    }
    std::shared_ptr<NativeMeasurementCache> NativeMeasurementCache::Create() noexcept
    {
        try
        {
            size_t actual{};
            auto value = std::allocate_shared<NativeMeasurementCache>(CacheAllocator<NativeMeasurementCache>{&actual},
                                                                      ConstructionKey{});
            value->_nativeBytes = actual;
            if (!value->_cache)
                return {};
            return value;
        }
        catch (...)
        {
            return {};
        }
    }
    void NativeMeasurementCache::Clear() noexcept
    {
        auto result = Pod<notepads_markdown_metric_cache_stats>();
        (void)notepads_markdown_metric_cache_clear(_cache, &result);
        _releaseWorkspace.store(true, std::memory_order_release);
        // Clear never waits for an in-flight shaper. Its workspace lease will
        // release optional storage after all borrowed spans have retired.
        TryReleaseWorkspace();
    }
    void NativeMeasurementCache::TryReleaseWorkspace() noexcept
    {
        if (!_releaseWorkspace.load(std::memory_order_acquire))
            return;
        try
        {
            std::unique_lock lock(_workspaceMutex, std::try_to_lock);
            if (lock.owns_lock() && !_workspaceLeased)
                ReleaseWorkspaceIfRequested();
        }
        catch (...)
        {
            // The current lease or a later successful acquisition still retires it.
        }
    }
    notepads_markdown_metric_cache_stats NativeMeasurementCache::Stats() const noexcept
    {
        auto result = Pod<notepads_markdown_metric_cache_stats>();
        if (notepads_markdown_metric_cache_get_stats(_cache, &result) != 0)
            return Pod<notepads_markdown_metric_cache_stats>();
        return result;
    }
    NativeMeasurementCache::Workspace NativeMeasurementCache::TryWorkspace() noexcept
    {
        Workspace result;
        try
        {
            result._lock = std::unique_lock<std::recursive_mutex>(_workspaceMutex, std::try_to_lock);
            if (result._lock.owns_lock())
            {
                if (_workspaceLeased)
                {
                    result._lock = {};
                    return result;
                }
                ReleaseWorkspaceIfRequested();
                _workspaceLeased = true;
                if (!_workspace)
                {
                    _workspace = std::make_unique<WorkspaceStorage>();
                    _workspaceBytes.store(sizeof(WorkspaceStorage), std::memory_order_relaxed);
                }
                result._owner = this;
            }
        }
        catch (...)
        {
            if (result._lock.owns_lock())
                _workspaceLeased = false;
            result._lock = {};
            TryReleaseWorkspace();
        }
        return result;
    }
    std::span<uint8_t> NativeMeasurementCache::Workspace::Key() noexcept
    {
        return _owner->_workspace->key;
    }
    std::span<uint8_t> NativeMeasurementCache::Workspace::Payload() noexcept
    {
        return _owner->_workspace->payload;
    }
    notepads_markdown_metric_cache_result NativeMeasurementCache::Workspace::Lookup(uint32_t keyBytes, uint64_t expectedEpoch) noexcept
    {
        auto result = Pod<notepads_markdown_metric_cache_result>();
        auto request = Pod<notepads_markdown_metric_cache_lookup_request>();
        request.expected_epoch = expectedEpoch;
        request.key_bytes = keyBytes;
        request.output_capacity = static_cast<uint32_t>(Payload().size());
        if (notepads_markdown_metric_cache_lookup(_owner->_cache, &request, Key().data(), Payload().data(), &result) !=
            0)
            return Pod<notepads_markdown_metric_cache_result>();
        return result;
    }
    void NativeMeasurementCache::Workspace::Put(uint32_t keyBytes, uint32_t valueBytes, uint64_t epoch) noexcept
    {
        auto request = Pod<notepads_markdown_metric_cache_put_request>();
        request.expected_epoch = epoch;
        request.key_bytes = keyBytes;
        request.value_bytes = valueBytes;
        auto result = Pod<notepads_markdown_metric_cache_result>();
        (void)notepads_markdown_metric_cache_put(_owner->_cache, &request, Key().data(), Payload().data(), &result);
    }

    void NativeMeasurementCache::ReleaseWorkspaceIfRequested() noexcept
    {
        if (_releaseWorkspace.exchange(false, std::memory_order_acq_rel))
        {
            _workspace.reset();
            _workspaceBytes.store(0, std::memory_order_relaxed);
        }
    }
    NativeMeasurementCache::Workspace::Workspace(Workspace &&other) noexcept
        : _owner(std::exchange(other._owner, nullptr)), _lock(std::move(other._lock))
    {
    }
    NativeMeasurementCache::Workspace &NativeMeasurementCache::Workspace::operator=(Workspace &&other) noexcept
    {
        if (this != &other)
        {
            Release();
            _owner = std::exchange(other._owner, nullptr);
            _lock = std::move(other._lock);
        }
        return *this;
    }
    NativeMeasurementCache::Workspace::~Workspace()
    {
        Release();
    }
    void NativeMeasurementCache::Workspace::Release() noexcept
    {
        auto owner = _owner;
        if (_owner && _lock.owns_lock())
        {
            _owner->ReleaseWorkspaceIfRequested();
            _owner->_workspaceLeased = false;
        }
        _owner = nullptr;
        _lock = {};
        // A concurrent Clear may have set its request after the last in-lock
        // check. Retry only after unlocking; a newer lease otherwise retires it.
        if (owner)
            owner->TryReleaseWorkspace();
    }
}
