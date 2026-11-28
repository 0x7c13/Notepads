// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Rendering/NativeMeasurementCache.h"

namespace winrt::Notepads::Markdown::implementation
{
    std::shared_ptr<::Notepads::Markdown::Native::NativeMeasurementCache> MarkdownView::EnsureMeasurementCache() noexcept
    {
        if (!_measurementCacheAttempted)
        {
            _measurementCacheAttempted = true;
            _measurementCache = ::Notepads::Markdown::Native::NativeMeasurementCache::Create();
        }
        return _measurementCache;
    }

    void MarkdownView::ClearMeasurementCache() noexcept
    {
        if (_measurementCache)
            _measurementCache->Clear();
    }

    Markdown::MarkdownMeasurementStatistics MarkdownView::GetMeasurementStatistics() const
    {
        CheckUiThread();
        Markdown::MarkdownMeasurementStatistics result{};
        if (_measurementCache)
        {
            auto stats = _measurementCache->Stats();
            result.Epoch = stats.epoch;
            result.HitCount = stats.hits;
            result.MissCount = stats.misses;
            result.EntryCount = stats.entries;
            result.ChargedBytes = stats.allocated_bytes + _measurementCache->NativeBytes();
            result.EvictionCount = stats.evictions;
        }
        return result;
    }
}
