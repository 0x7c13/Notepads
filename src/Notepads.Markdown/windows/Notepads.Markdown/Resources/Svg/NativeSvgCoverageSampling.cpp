// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "Resources/Svg/NativeSvgCoverageSampling.h"
#include "Resources/Svg/NativeSvgFontMemory.h"
#include <bit>
#include <numeric>

namespace Notepads::Markdown::Native
{
    using SvgFontMemory::Quota;
    namespace
    {
        constexpr uint64_t NumeratorLimit = uint64_t{1} << 56;
        constexpr uint64_t DenominatorLimit = uint64_t{1} << 24;
        constexpr uint64_t PixelWindow = 4096;

        SvgCoverageWide Wide(uint64_t value) noexcept
        {
            return {{static_cast<uint32_t>(value), static_cast<uint32_t>(value >> 32), 0, 0}};
        }

        int Compare(SvgCoverageWide const& a, SvgCoverageWide const& b) noexcept
        {
            for (unsigned i = 4; i != 0; --i)
            {
                if (a.limbs[i - 1] != b.limbs[i - 1])
                    return a.limbs[i - 1] < b.limbs[i - 1] ? -1 : 1;
            }
            return 0;
        }

        SvgCoverageWide Multiply(uint64_t a, uint64_t b) noexcept
        {
            // Every inner sum is at most (2^32-1)^2 + 2*(2^32-1),
            // exactly UINT64_MAX. The final product has four limbs.
            SvgCoverageWide result{};
            uint32_t const left[2]{static_cast<uint32_t>(a), static_cast<uint32_t>(a >> 32)};
            uint32_t const right[2]{static_cast<uint32_t>(b), static_cast<uint32_t>(b >> 32)};
            for (unsigned i = 0; i != 2; ++i)
            {
                uint64_t carry = 0;
                for (unsigned j = 0; j != 2; ++j)
                {
                    uint64_t value = uint64_t{left[i]} * right[j] + result.limbs[i + j] + carry;
                    result.limbs[i + j] = static_cast<uint32_t>(value);
                    carry = value >> 32;
                }
                result.limbs[i + 2] = static_cast<uint32_t>(carry);
            }
            return result;
        }

        SvgCoverageWide MultiplySmall(SvgCoverageWide const& a, uint32_t factor)
        {
            SvgCoverageWide result{};
            uint64_t carry = 0;
            for (unsigned i = 0; i != 4; ++i)
            {
                uint64_t value = uint64_t{a.limbs[i]} * factor + carry;
                result.limbs[i] = static_cast<uint32_t>(value);
                carry = value >> 32;
            }
            if (carry)
                throw E_INVALIDARG;
            return result;
        }

        SvgCoverageWide Add(SvgCoverageWide const& a, SvgCoverageWide const& b)
        {
            SvgCoverageWide result{};
            uint64_t carry = 0;
            for (unsigned i = 0; i != 4; ++i)
            {
                uint64_t value = uint64_t{a.limbs[i]} + b.limbs[i] + carry;
                result.limbs[i] = static_cast<uint32_t>(value);
                carry = value >> 32;
            }
            if (carry)
                throw E_INVALIDARG;
            return result;
        }

        SvgCoverageWide Half(SvgCoverageWide const& value) noexcept
        {
            SvgCoverageWide result{};
            for (unsigned i = 0; i != 4; ++i)
            {
                uint32_t high = i == 3 ? 0 : value.limbs[i + 1];
                result.limbs[i] = (value.limbs[i] >> 1) | (high << 31);
            }
            return result;
        }

        bool Extent(uint32_t target, uint64_t numerator, uint64_t denominator, uint32_t& result) noexcept
        {
            // Test before target*N: UINT32_MAX*D < 2^56. Passing this
            // admission also proves every later axis endpoint fits uint64_t.
            uint64_t limit = uint64_t{UINT32_MAX} * denominator;
            if (target && numerator > limit / target)
                return false;
            uint64_t product = uint64_t{target} * numerator;
            result = static_cast<uint32_t>(product / denominator + (product % denominator != 0));
            return true;
        }

        bool ValidatePlan(SvgCoverageSampling const& plan)
        {
            if (!plan.target_width || !plan.target_height)
            {
                // Canonical empty sentinel deliberately has no ratio: a legal
                // empty request may have an otherwise unrepresentable s/t.
                if (plan.numerator || plan.denominator || plan.source_width || plan.source_height ||
                    Compare(plan.area, Wide(0)) != 0)
                    throw E_INVALIDARG;
                return false;
            }
            if (!plan.denominator || plan.denominator >= DenominatorLimit ||
                plan.numerator < plan.denominator || plan.numerator >= NumeratorLimit)
                throw E_INVALIDARG;
            uint32_t width{}, height{};
            if (!Extent(plan.target_width, plan.numerator, plan.denominator, width) ||
                !Extent(plan.target_height, plan.numerator, plan.denominator, height) ||
                width != plan.source_width || height != plan.source_height ||
                Compare(plan.area, Multiply(plan.numerator, plan.numerator)) != 0)
                throw E_INVALIDARG;
            return true;
        }

        uint32_t ValidateBlock(SvgCoverageSampling const& plan, SvgCoverageTargetBlock const& block,
                               size_t sums_size, void const* sums_data)
        {
            if (block.width > 16 || block.height > 16 ||
                uint64_t{block.x} + block.width > plan.target_width ||
                uint64_t{block.y} + block.height > plan.target_height)
                throw E_INVALIDARG;
            uint32_t count = block.width * block.height;
            if (sums_size < count)
                throw Quota;
            if (count && !sums_data)
                throw E_INVALIDARG;
            return count;
        }

        uint64_t Payload(uint32_t width, uint32_t height, uint64_t stride, uint32_t channels)
        {
            if (!width || !height)
                return 0;
            uint64_t row = uint64_t{width} * channels;
            if (stride < row)
                throw E_INVALIDARG;
            uint64_t rows = uint64_t{height} - 1;
            if (rows && stride > (UINT64_MAX - row) / rows)
                throw Quota;
            return rows * stride + row;
        }

        void ValidateWindow(SvgCoverageSampling const& plan, SvgCoverageSourceWindow const& window)
        {
            if (uint64_t{window.x} + window.width > plan.source_width ||
                uint64_t{window.y} + window.height > plan.source_height)
                throw E_INVALIDARG;
            uint64_t bytes = Payload(window.width, window.height, window.stride, 4);
            if (bytes > window.bgra.size())
                throw Quota;
            if (bytes && !window.bgra.data())
                throw E_INVALIDARG;
        }

        struct Axis
        {
            uint64_t first{}, end{}, lower{}, upper{};
        };

        Axis Intersect(uint32_t cell, uint32_t origin, uint32_t length,
                       uint64_t numerator, uint64_t denominator) noexcept
        {
            uint64_t lower = uint64_t{cell} * numerator;
            uint64_t upper = (uint64_t{cell} + 1) * numerator;
            uint64_t first = lower / denominator;
            uint64_t end = upper / denominator + (upper % denominator != 0);
            uint64_t window_end = uint64_t{origin} + length;
            first = first < origin ? origin : first;
            end = end > window_end ? window_end : end;
            if (end < first)
                end = first;
            return {first, end, lower, upper};
        }

        uint64_t Weight(Axis const& axis, uint64_t sample, uint64_t denominator) noexcept
        {
            uint64_t lower = sample * denominator;
            uint64_t upper = (sample + 1) * denominator;
            lower = lower < axis.lower ? axis.lower : lower;
            upper = upper > axis.upper ? axis.upper : upper;
            return upper - lower;
        }
    }

    SvgCoverageSampling PlanSvgCoverageSampling(uint32_t s_bits, uint32_t t_bits,
                                                uint32_t target_width, uint32_t target_height)
    {
        auto positive_finite = [](uint32_t bits) noexcept
        {
            return bits && !(bits & 0x80000000u) && (bits & 0x7f800000u) != 0x7f800000u;
        };
        if (!positive_finite(s_bits) || !positive_finite(t_bits) || s_bits < t_bits)
            throw E_INVALIDARG;

        SvgCoverageSampling result{};
        result.target_width = target_width;
        result.target_height = target_height;
        if (!target_width || !target_height)
            return result;

        uint32_t s_exponent = s_bits >> 23;
        uint32_t t_exponent = t_bits >> 23;
        uint32_t a = (s_bits & 0x7fffffu) | (s_exponent ? 0x800000u : 0);
        uint32_t b = (t_bits & 0x7fffffu) | (t_exponent ? 0x800000u : 0);
        // Normal values use exponent-150; subnormals use -149. Thus
        // positive s>=t proves this raw exponent difference nonnegative,
        // including the subnormal/normal boundary (both have power -149).
        uint32_t shift = (s_exponent ? s_exponent : 1) - (t_exponent ? t_exponent : 1);
        uint32_t common = std::gcd(a, b);
        a /= common;
        b /= common;
        uint32_t cancel = std::countr_zero(b);
        cancel = cancel < shift ? cancel : shift;
        b >>= cancel;
        shift -= cancel;

        // No potentially large shift happens before logical admission. If
        // N needs more than 56 bits, N/D > UINT32_MAX for every nonempty
        // target because D<2^24, so this is a logical extent Quota.
        if (shift > 56 || std::bit_width(a) + shift > 56)
            throw Quota;
        result.numerator = uint64_t{a} << shift;
        result.denominator = b;
        if (!Extent(target_width, result.numerator, result.denominator, result.source_width) ||
            !Extent(target_height, result.numerator, result.denominator, result.source_height))
            throw Quota;
        result.area = Multiply(result.numerator, result.numerator);
        return result;
    }

    void AccumulateSvgCoverageWindow(SvgCoverageSampling const& plan,
                                    SvgCoverageTargetBlock const& block,
                                    SvgCoverageSourceWindow const& window,
                                    std::span<SvgCoverageWide> sums,
                                    SvgCoverageCheckpoint checkpoint, void* context)
    {
        if (!checkpoint)
            throw E_INVALIDARG;
        bool nonempty = ValidatePlan(plan);
        uint32_t cells = ValidateBlock(plan, block, sums.size(), sums.data());
        ValidateWindow(plan, window);
        checkpoint(context, 0);
        if (!nonempty || !cells)
        {
            checkpoint(context, 0);
            return;
        }

        // One prepaid <=256-cell planning pass validates accumulators and
        // counts all sample contributions. Geometry is recomputed below;
        // no per-cell range array or hidden allocation is required.
        checkpoint(context, cells);
        SvgCoverageWide maximum = MultiplySmall(plan.area, 255);
        uint64_t contributions = 0;
        for (uint32_t y = 0; y != block.height; ++y)
        {
            Axis vertical = Intersect(block.y + y, window.y, window.height, plan.numerator, plan.denominator);
            for (uint32_t x = 0; x != block.width; ++x)
            {
                if (Compare(sums[y * block.width + x], maximum) > 0)
                    throw E_INVALIDARG;
                Axis horizontal = Intersect(block.x + x, window.x, window.width, plan.numerator, plan.denominator);
                uint64_t width = horizontal.end - horizontal.first;
                uint64_t height = vertical.end - vertical.first;
                if (height && width > (UINT64_MAX - contributions) / height)
                    throw Quota;
                contributions += width * height;
            }
        }

        uint64_t remaining = contributions;
        uint64_t credit = 0;
        for (uint32_t y = 0; y != block.height; ++y)
        {
            Axis vertical = Intersect(block.y + y, window.y, window.height, plan.numerator, plan.denominator);
            for (uint32_t x = 0; x != block.width; ++x)
            {
                Axis horizontal = Intersect(block.x + x, window.x, window.width, plan.numerator, plan.denominator);
                SvgCoverageWide sum = sums[y * block.width + x];
                if (horizontal.first == horizontal.end || vertical.first == vertical.end)
                    continue;
                for (uint64_t sy = vertical.first; sy != vertical.end; ++sy)
                {
                    uint64_t wy = Weight(vertical, sy, plan.denominator);
                    uint64_t row = (sy - window.y) * window.stride;
                    for (uint64_t sx = horizontal.first; sx != horizontal.end; ++sx)
                    {
                        if (!credit)
                        {
                            credit = remaining < PixelWindow ? remaining : PixelWindow;
                            checkpoint(context, credit);
                        }
                        --credit;
                        --remaining;
                        uint64_t wx = Weight(horizontal, sx, plan.denominator);
                        size_t offset = static_cast<size_t>(row + (sx - window.x) * 4 + 3);
                        uint64_t contribution = uint64_t{window.bgra[offset]} * wx * wy;
                        // wx,wy<=D<2^24, hence this single term is <2^56.
                        sum = Add(sum, Wide(contribution));
                    }
                }
                if (Compare(sum, maximum) > 0)
                    throw E_INVALIDARG;
                sums[y * block.width + x] = sum;
            }
        }
        checkpoint(context, 0);
    }

    void ResolveSvgCoverageBlock(SvgCoverageSampling const& plan,
                                 SvgCoverageTargetBlock const& block,
                                 std::span<SvgCoverageWide const> sums,
                                 std::span<uint8_t> target, uint64_t stride,
                                 SvgCoverageCheckpoint checkpoint, void* context)
    {
        if (!checkpoint)
            throw E_INVALIDARG;
        bool nonempty = ValidatePlan(plan);
        uint32_t cells = ValidateBlock(plan, block, sums.size(), sums.data());
        uint64_t bytes = Payload(plan.target_width, plan.target_height, stride, 1);
        if (bytes > target.size())
            throw Quota;
        if (bytes && !target.data())
            throw E_INVALIDARG;
        checkpoint(context, 0);
        if (!nonempty || !cells)
        {
            checkpoint(context, 0);
            return;
        }

        checkpoint(context, cells);
        SvgCoverageWide maximum = MultiplySmall(plan.area, 255);
        SvgCoverageWide half = Half(plan.area);
        for (uint32_t y = 0; y != block.height; ++y)
        {
            for (uint32_t x = 0; x != block.width; ++x)
            {
                SvgCoverageWide sum = sums[y * block.width + x];
                if (Compare(sum, maximum) > 0)
                    throw E_INVALIDARG;
                SvgCoverageWide rounded = Add(sum, half);
                uint32_t value = 0;
                // Exact quotient is in [0,255]. Use exactly eight bounded
                // comparisons instead of general division or floating point.
                for (uint32_t bit = 128; bit; bit >>= 1)
                {
                    uint32_t candidate = value | bit;
                    if (Compare(MultiplySmall(plan.area, candidate), rounded) <= 0)
                        value = candidate;
                }
                size_t offset = static_cast<size_t>(uint64_t{block.y + y} * stride + block.x + x);
                target[offset] = static_cast<uint8_t>(value);
            }
        }
        checkpoint(context, 0);
    }
}
