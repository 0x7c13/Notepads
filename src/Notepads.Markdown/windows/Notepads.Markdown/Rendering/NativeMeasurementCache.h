// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/measurement_cache.h"
#include <array>
#include <atomic>
#include <memory>
#include <mutex>
#include <span>

namespace Notepads::Markdown::Native
{
    struct TextMetricLine
    {
        uint32_t length{}, newlineLength{};
        float height{};
    };

    // Borrowed only during a workspace lease. No platform struct/pointer is serialized.
    class TextMetricView
    {
      public:
        explicit TextMetricView(std::span<uint8_t const> bytes = {}) noexcept : _bytes(bytes)
        {
        }
        bool Valid(uint32_t expectedLength) const noexcept;
        uint32_t Length() const noexcept;
        uint32_t ClusterCount() const noexcept;
        uint32_t LineCount() const noexcept;
        uint32_t Cluster(uint32_t index) const noexcept;
        TextMetricLine Line(uint32_t index) const noexcept;
        float Height() const noexcept;
        float Width() const noexcept;
        float OverflowAbove() const noexcept;
        float OverflowBelow() const noexcept;
        static constexpr uint32_t Magic = 0x314D534Du;
        static constexpr uint32_t HeaderBytes = 40;

      private:
        std::span<uint8_t const> _bytes;
        uint32_t Word(size_t offset) const noexcept;
    };

    class NativeMeasurementCache final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<NativeMeasurementCache> Create() noexcept;
        explicit NativeMeasurementCache(ConstructionKey) noexcept;
        ~NativeMeasurementCache();
        void Clear() noexcept;
        notepads_markdown_metric_cache_stats Stats() const noexcept;
        uint64_t NativeBytes() const noexcept
        {
            return _nativeBytes + _workspaceBytes.load(std::memory_order_relaxed);
        }

        class Workspace
        {
          public:
            Workspace() noexcept = default;
            Workspace(Workspace &&other) noexcept;
            Workspace &operator=(Workspace &&other) noexcept;
            ~Workspace();
            explicit operator bool() const noexcept
            {
                return _lock.owns_lock();
            }
            std::span<uint8_t> Key() noexcept;
            std::span<uint8_t> Payload() noexcept;
            notepads_markdown_metric_cache_result Lookup(uint32_t keyBytes, uint64_t expectedEpoch) noexcept;
            void Put(uint32_t keyBytes, uint32_t valueBytes, uint64_t epoch) noexcept;

          private:
            friend class NativeMeasurementCache;
            NativeMeasurementCache *_owner{};
            std::unique_lock<std::recursive_mutex> _lock;
            void Release() noexcept;
        };
        Workspace TryWorkspace() noexcept;

      private:
        ModuleLifetime _module;
        notepads_markdown_metric_cache *_cache{};
        uint64_t _nativeBytes{};
        std::recursive_mutex _workspaceMutex;
        struct WorkspaceStorage
        {
            alignas(8) std::array<uint8_t, NOTEPADS_MARKDOWN_METRIC_CACHE_KEY_LIMIT> key;
            alignas(8) std::array<uint8_t, NOTEPADS_MARKDOWN_METRIC_CACHE_ENTRY_BYTES_LIMIT> payload;
        };
        std::unique_ptr<WorkspaceStorage> _workspace;
        std::atomic<uint64_t> _workspaceBytes{};
        std::atomic<bool> _releaseWorkspace{};
        bool _workspaceLeased{};
        void ReleaseWorkspaceIfRequested() noexcept;
        void TryReleaseWorkspace() noexcept;
    };
}
