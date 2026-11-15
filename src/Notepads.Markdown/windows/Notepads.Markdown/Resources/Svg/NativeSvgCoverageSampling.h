// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <cstdint>
#include <span>

namespace Notepads::Markdown::Native
{
    struct SvgCoverageWide
    {
        uint32_t limbs[4]{};
    };
    struct SvgCoverageSampling
    {
        uint64_t numerator{}, denominator{};
        SvgCoverageWide area{};
        uint32_t target_width{}, target_height{}, source_width{}, source_height{};
    };
    struct SvgCoverageTargetBlock
    {
        uint32_t x{}, y{}, width{}, height{};
    };
    struct SvgCoverageSourceWindow
    {
        uint32_t x{}, y{}, width{}, height{};
        uint64_t stride{};
        std::span<uint8_t const> bgra;
    };
    using SvgCoverageCheckpoint = void (*)(void*, uint64_t);

    // Exact ratio of original positive finite s/t, not a divided f32 estimate.
    // Nonempty source logical extents must fit UINT32_MAX before shifts or loops.
    // Invalid metadata throws E_INVALIDARG; extent/capacity failure throws Quota.
    // If either target dimension is zero, preserve both target dimensions and
    // return a canonical empty sentinel: N/D/area/source extents all zero.
    // Accumulate/Resolve accept that sentinel only as empty and never divide by it.
    SvgCoverageSampling PlanSvgCoverageSampling(uint32_t s_bits, uint32_t t_bits,
                                                uint32_t target_width, uint32_t target_height);

    // No storage allocation or ownership is acquired. The caller zeros and
    // pays for width*height accumulators before the first disjoint source tile.
    // Target blocks are at most 16x16; source windows carry global sample origin.
    // The caller partitions each required source region without overlap or gaps.
    // BGRA alpha is already sample-cell coverage; do not clip ceil cells again.
    // Pixel reads and writes are prepaid in windows of at most 4096 operations.
    void AccumulateSvgCoverageWindow(SvgCoverageSampling const& plan,
                                    SvgCoverageTargetBlock const& block,
                                    SvgCoverageSourceWindow const& window,
                                    std::span<SvgCoverageWide> sums,
                                    SvgCoverageCheckpoint checkpoint, void* context);

    // Exactly one half-up rounding per completed target cell, denominator N^2.
    // Write only the requested block in the caller's whole-target A8 surface.
    // No intermediate normalization/rounding occurs between source windows.
    void ResolveSvgCoverageBlock(SvgCoverageSampling const& plan,
                                 SvgCoverageTargetBlock const& block,
                                 std::span<SvgCoverageWide const> sums,
                                 std::span<uint8_t> target, uint64_t stride,
                                 SvgCoverageCheckpoint checkpoint, void* context);
}
