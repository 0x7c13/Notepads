// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "Resources/Svg/NativeSvgCoverageFont.h"
#include <algorithm>
#include <bit>
#include <cfenv>
#include <cmath>
#include <limits>
#include <string_view>

// The interval operations and the Type2 absolute-to-relative conversion require
// ordinary IEEE arithmetic, without reassociation or contraction.
#pragma float_control(precise, on, push)

namespace Notepads::Markdown::Native
{
    using SvgFontMemory::Font;
    using SvgFontMemory::Check;
    using SvgFontMemory::Bytes;
    namespace
    {
        constexpr double Infinity = std::numeric_limits<double>::infinity();
        constexpr uint32_t CoordinateDomain = NOTEPADS_MARKDOWN_SVG_COVERAGE_COORDINATE_DOMAIN;
        constexpr uint32_t PrecisionDomain = NOTEPADS_MARKDOWN_SVG_COVERAGE_PRECISION_DOMAIN;
        constexpr uint32_t FormatDomain = NOTEPADS_MARKDOWN_SVG_COVERAGE_FONT_FORMAT_DOMAIN;
        constexpr uint32_t UnitsPerEm = 2048;
        constexpr uint32_t MaximumCharStringBytes = 65535;
        constexpr std::string_view PostScriptName = "NotepadsMarkdownSvgCoverageV1";
        constexpr std::string_view FamilyName = "Notepads Markdown SVG Coverage V1";
        constexpr std::string_view GlyphNames[] = {
            "p01", "p02", "p03", "p04", "p05", "p06", "p07", "p08",
            "p09", "p10", "p11", "p12", "p13", "p14", "p15", "p16",
            "p17", "p18", "p19", "p20", "p21", "p22", "p23", "p24",
            "p25", "p26", "p27", "p28", "p29", "p30", "p31", "p32"};

        struct RepresentationDomain
        {
            uint32_t reason;
        };

        void Work(SvgFontAllocationContext& memory, uint64_t amount)
        {
            Check(memory.services.checkpoint(memory.services.context, amount));
        }

        double Lower(double value) noexcept
        {
            return std::nextafter(value, -Infinity);
        }

        double Upper(double value) noexcept
        {
            return std::nextafter(value, Infinity);
        }

        // The residual of TwoSum is exact for the finite, non-overflowing
        // arithmetic here. In particular, exact original dyadic coordinates do
        // not acquire an artificial interval around an integer or a half tie.
        struct Interval
        {
            double lo{}, hi{};
            Interval() noexcept = default;
            explicit Interval(double value) noexcept : lo(value), hi(value)
            {
            }
            Interval(double low, double high) noexcept : lo(low), hi(high)
            {
            }
        };

        double SumResidual(double a, double b, double sum) noexcept
        {
            double z = sum - a;
            return (a - (sum - z)) + (b - z);
        }

        Interval Add(Interval a, Interval b) noexcept
        {
            double low = a.lo + b.lo;
            double high = a.hi + b.hi;
            if (SumResidual(a.lo, b.lo, low) < 0)
                low = Lower(low);
            if (SumResidual(a.hi, b.hi, high) > 0)
                high = Upper(high);
            return {low, high};
        }

        Interval Negate(Interval value) noexcept
        {
            return {-value.hi, -value.lo};
        }

        Interval Subtract(Interval a, Interval b) noexcept
        {
            return Add(a, Negate(b));
        }

        Interval Multiply(Interval a, Interval b) noexcept
        {
            double aa = a.lo * b.lo, ab = a.lo * b.hi;
            double ba = a.hi * b.lo, bb = a.hi * b.hi;
            double low = std::min({aa, ab, ba, bb});
            double high = std::max({aa, ab, ba, bb});
            // Keep exact point products exact. fma returns the actual product
            // residual, rather than testing a rounded reverse multiplication.
            if (a.lo == a.hi && b.lo == b.hi)
            {
                double residual = std::fma(a.lo, b.lo, -aa);
                return {residual < 0 ? Lower(aa) : aa, residual > 0 ? Upper(aa) : aa};
            }
            return {Lower(low), Upper(high)};
        }

        Interval Divide(Interval a, Interval b)
        {
            if (b.lo <= 0 && b.hi >= 0)
                throw RepresentationDomain{PrecisionDomain};
            double aa = a.lo / b.lo, ab = a.lo / b.hi;
            double ba = a.hi / b.lo, bb = a.hi / b.hi;
            if (a.lo == a.hi && b.lo == b.hi)
            {
                double residual = std::fma(-aa, b.lo, a.lo);
                if (b.lo < 0)
                    residual = -residual;
                return {residual < 0 ? Lower(aa) : aa, residual > 0 ? Upper(aa) : aa};
            }
            return {Lower(std::min({aa, ab, ba, bb})), Upper(std::max({aa, ab, ba, bb}))};
        }

        Interval Scale(Interval value, int exponent) noexcept
        {
            // All powers used here remain normal in double (the source domain
            // is f32 and the encoded domain is signed 16.16).
            return {std::ldexp(value.lo, exponent), std::ldexp(value.hi, exponent)};
        }

        double Magnitude(Interval value) noexcept
        {
            return std::max(std::abs(value.lo), std::abs(value.hi));
        }

        double DecodeFloat(uint32_t bits) noexcept
        {
            uint32_t exponent = (bits >> 23) & 255;
            uint32_t mantissa = bits & 0x7FFFFF;
            if (exponent)
                mantissa |= 0x800000;
            double value = std::ldexp(static_cast<double>(mantissa), exponent ? static_cast<int>(exponent) - 150 : -149);
            return bits >> 31 ? -value : value;
        }

        double DecodeFloat(float value) noexcept
        {
            return DecodeFloat(std::bit_cast<uint32_t>(value));
        }

        struct Point
        {
            Interval x, y;
        };

        Point SourcePoint(MsSvgCoverageCommand const& command, uint32_t point) noexcept
        {
            return {Interval(DecodeFloat(command.xy_bits[point * 2])),
                    Interval(DecodeFloat(command.xy_bits[point * 2 + 1]))};
        }

        Point QuadControl(Point endpoint, Point control)
        {
            // (P + 2Q)/3, including the division residual. This is an exact
            // rational enclosure, not an assumption that double is rational.
            return {Divide(Add(endpoint.x, Scale(control.x, 1)), Interval(3)),
                    Divide(Add(endpoint.y, Scale(control.y, 1)), Interval(3))};
        }

        struct FixedPoint
        {
            int32_t x{}, y{};
        };

        double RoundEven(double value) noexcept
        {
            double integral = std::floor(value);
            double fraction = value - integral;
            if (fraction > .5 || (fraction == .5 && std::fmod(integral, 2) != 0))
                integral += 1;
            return integral;
        }

        int32_t Fixed(Interval value)
        {
            Interval scaled = Scale(value, 16);
            double low = RoundEven(scaled.lo), high = RoundEven(scaled.hi);
            if (low != high)
                throw RepresentationDomain{PrecisionDomain};
            if (low < INT32_MIN || low > INT32_MAX)
                throw RepresentationDomain{CoordinateDomain};
            return static_cast<int32_t>(low);
        }

        struct Bounds
        {
            Interval min_x, min_y, max_x, max_y;
            bool present{};

            void Include(Interval x, Interval y) noexcept
            {
                if (!present)
                {
                    min_x = max_x = x;
                    min_y = max_y = y;
                    present = true;
                    return;
                }
                min_x = {std::min(min_x.lo, x.lo), std::min(min_x.hi, x.hi)};
                min_y = {std::min(min_y.lo, y.lo), std::min(min_y.hi, y.hi)};
                max_x = {std::max(max_x.lo, x.lo), std::max(max_x.hi, x.hi)};
                max_y = {std::max(max_y.lo, y.lo), std::max(max_y.hi, y.hi)};
            }

            void Include(FixedPoint value) noexcept
            {
                Include(Interval(value.x), Interval(value.y));
            }
        };

        Interval EvaluateCubic(int32_t p0, int32_t p1, int32_t p2, int32_t p3, Interval t) noexcept
        {
            Interval u = Subtract(Interval(1), t);
            auto lerp = [t, u](Interval a, Interval b)
            {
                return Add(Multiply(u, a), Multiply(t, b));
            };
            Interval a = lerp(Interval(p0), Interval(p1));
            Interval b = lerp(Interval(p1), Interval(p2));
            Interval c = lerp(Interval(p2), Interval(p3));
            return lerp(lerp(a, b), lerp(b, c));
        }

        // Bounds use actual quantized curves. Ambiguous integer extrema are
        // rejected instead of putting a guessed LSB into hmtx. Exact dyadic
        // extrema, including the common symmetric half point, remain exact.
        void CubicAxis(Bounds& bounds, FixedPoint p0, FixedPoint p1, FixedPoint p2, FixedPoint p3, bool x_axis)
        {
            auto axis = [x_axis](FixedPoint p) -> int64_t { return x_axis ? p.x : p.y; };
            int64_t v0 = axis(p0), v1 = axis(p1), v2 = axis(p2), v3 = axis(p3);
            int64_t a = -v0 + 3 * v1 - 3 * v2 + v3;
            int64_t b = 2 * (v0 - 2 * v1 + v2);
            int64_t c = v1 - v0;
            std::array<Interval, 2> roots{};
            uint32_t count{};
            if (!a)
            {
                if (b)
                    roots[count++] = Divide(Interval(-static_cast<double>(c)), Interval(static_cast<double>(b)));
            }
            else
            {
                double bb = static_cast<double>(b) * static_cast<double>(b);
                double ac = 4 * static_cast<double>(a) * static_cast<double>(c);
                double bb_error = std::fma(static_cast<double>(b), static_cast<double>(b), -bb);
                double ac_error = std::fma(4 * static_cast<double>(a), static_cast<double>(c), -ac);
                // Difference of exact two-term products also resolves an exact
                // zero discriminant without subtracting widened intervals.
                Interval discriminant = Add(Subtract(Interval(bb), Interval(ac)),
                                            Subtract(Interval(bb_error), Interval(ac_error)));
                if (discriminant.hi < 0)
                    return;
                if (discriminant.lo < 0)
                    throw RepresentationDomain{PrecisionDomain};
                Interval square_root;
                if (discriminant.lo == 0 && discriminant.hi == 0)
                    square_root = Interval(0);
                else
                    square_root = {Lower(std::sqrt(discriminant.lo)), Upper(std::sqrt(discriminant.hi))};
                if (b < 0)
                    square_root = Negate(square_root);
                Interval q = Scale(Negate(Add(Interval(static_cast<double>(b)), square_root)), -1);
                if (q.lo <= 0 && q.hi >= 0)
                {
                    // A*t*t is the only remaining derivative when q is zero.
                    if (b || c)
                        throw RepresentationDomain{PrecisionDomain};
                    return;
                }
                roots[count++] = Divide(q, Interval(static_cast<double>(a)));
                roots[count++] = Divide(Interval(static_cast<double>(c)), q);
            }
            for (uint32_t i = 0; i < count; ++i)
            {
                Interval t = roots[i];
                if (t.hi <= 0 || t.lo >= 1)
                    continue;
                if ((t.lo <= 0 && t.hi >= 0 && c == 0) ||
                    (t.lo <= 1 && t.hi >= 1 && a + b + c == 0))
                    continue; // exact boundary stationary points are endpoints
                if (t.lo <= .5 && t.hi >= .5 && a + 2 * b + 4 * c == 0)
                    t = Interval(.5);
                t = {std::max(0.0, t.lo), std::min(1.0, t.hi)};
                Interval value = EvaluateCubic(static_cast<int32_t>(v0), static_cast<int32_t>(v1),
                                               static_cast<int32_t>(v2), static_cast<int32_t>(v3), t);
                // The curve also lies in its exact control hull. Intersecting
                // that independent bound avoids a false extra integer at an
                // endpoint shared with a horizontal/vertical tangent.
                value.lo = std::max(value.lo, static_cast<double>(std::min({v0, v1, v2, v3})));
                value.hi = std::min(value.hi, static_cast<double>(std::max({v0, v1, v2, v3})));
                if (x_axis)
                {
                    bounds.min_x = {std::min(bounds.min_x.lo, value.lo), std::min(bounds.min_x.hi, value.hi)};
                    bounds.max_x = {std::max(bounds.max_x.lo, value.lo), std::max(bounds.max_x.hi, value.hi)};
                }
                else
                {
                    bounds.min_y = {std::min(bounds.min_y.lo, value.lo), std::min(bounds.min_y.hi, value.hi)};
                    bounds.max_y = {std::max(bounds.max_y.lo, value.lo), std::max(bounds.max_y.hi, value.hi)};
                }
            }
        }

        int16_t BoundInteger(Interval value, bool maximum)
        {
            value = Scale(value, -16);
            double low = maximum ? std::ceil(value.lo) : std::floor(value.lo);
            double high = maximum ? std::ceil(value.hi) : std::floor(value.hi);
            if (low != high)
                throw RepresentationDomain{PrecisionDomain};
            if (low < INT16_MIN || low > INT16_MAX)
                throw RepresentationDomain{FormatDomain};
            return static_cast<int16_t>(low);
        }

        struct GlyphPlan
        {
            float baseline_x{}, baseline_y{}, em_size{};
            int exponent{};
            uint32_t input{}, bytes{};
            uint16_t gid{};
            std::array<int16_t, 4> box{};
            double project_x{}, project_y{};
        };

        FixedPoint Encode(Point source, GlyphPlan const& plan)
        {
            FixedPoint result{
                Fixed(Scale(Subtract(source.x, Interval(DecodeFloat(plan.baseline_x))), plan.exponent)),
                Fixed(Negate(Scale(Subtract(source.y, Interval(DecodeFloat(plan.baseline_y))), plan.exponent)))};
            // Include conversion of the actual 16.16 absolute point to Native
            // f32, em/UPM scaling, and f32 baseline restoration. em and k are
            // powers of two and their actual f32 values are used here.
            float dx = std::ldexp(static_cast<float>(std::ldexp(static_cast<double>(result.x), -16)), -plan.exponent);
            float dy = -std::ldexp(static_cast<float>(std::ldexp(static_cast<double>(result.y), -16)), -plan.exponent);
            float restored_x = plan.baseline_x + dx;
            float restored_y = plan.baseline_y + dy;
            if (!std::isfinite(dx) || !std::isfinite(dy) || !std::isfinite(restored_x) || !std::isfinite(restored_y))
                throw RepresentationDomain{CoordinateDomain};
            double ex = Magnitude(Subtract(Interval(DecodeFloat(restored_x)), source.x));
            double ey = Magnitude(Subtract(Interval(DecodeFloat(restored_y)), source.y));
            // The L1 bound also bounds Euclidean sample error. Matrix rounding
            // and optional Outline have their own separate 1/4096 allowances.
            double projected = Upper(Upper(plan.project_x * ex) + Upper(plan.project_y * ey));
            if (projected > 1.0 / 2048)
                throw RepresentationDomain{PrecisionDomain};
            return result;
        }

        class Sink
        {
            SvgFontAllocationContext* _memory{};
            uint8_t* _data{};
            uint64_t _size{}, _position{}, _credit{};
        public:
            Sink() noexcept = default;
            Sink(SvgFontAllocationContext& memory, uint8_t* data, uint64_t size) noexcept
                : _memory(&memory), _data(data), _size(size)
            {
            }
            uint32_t Position() const
            {
                if (_position > UINT32_MAX)
                    throw RepresentationDomain{FormatDomain};
                return static_cast<uint32_t>(_position);
            }
            void Byte(uint8_t value)
            {
                if (_position == UINT32_MAX)
                    throw RepresentationDomain{FormatDomain};
                if (_data)
                {
                    if (_position >= _size)
                        throw E_UNEXPECTED;
                    if (!_credit)
                    {
                        _credit = std::min<uint64_t>(4096, _size - _position);
                        Work(*_memory, _credit);
                    }
                    _data[static_cast<size_t>(_position)] = value;
                    --_credit;
                }
                ++_position;
            }
            void U16(uint16_t value)
            {
                Byte(static_cast<uint8_t>(value >> 8));
                Byte(static_cast<uint8_t>(value));
            }
            void U32(uint32_t value)
            {
                U16(static_cast<uint16_t>(value >> 16));
                U16(static_cast<uint16_t>(value));
            }
            void Zeros(uint32_t count)
            {
                for (uint32_t i = 0; i < count; ++i)
                    Byte(0);
            }
            void Ascii(std::string_view value)
            {
                for (char c : value)
                    Byte(static_cast<uint8_t>(c));
            }
            void Utf16(std::string_view value)
            {
                for (char c : value)
                    U16(static_cast<uint8_t>(c));
            }
            void Finish(uint32_t expected) const
            {
                if (_position != expected)
                    throw E_UNEXPECTED;
            }
        };

        void Operand(Sink& sink, int64_t fixed)
        {
            if (fixed < INT32_MIN || fixed > INT32_MAX)
                throw RepresentationDomain{CoordinateDomain};
            if (fixed % 65536 == 0)
            {
                int32_t value = static_cast<int32_t>(fixed / 65536);
                if (value >= -107 && value <= 107)
                {
                    sink.Byte(static_cast<uint8_t>(value + 139));
                    return;
                }
                if (value >= 108 && value <= 1131)
                {
                    uint32_t n = static_cast<uint32_t>(value - 108);
                    sink.Byte(static_cast<uint8_t>(247 + n / 256));
                    sink.Byte(static_cast<uint8_t>(n));
                    return;
                }
                if (value >= -1131 && value <= -108)
                {
                    uint32_t n = static_cast<uint32_t>(-value - 108);
                    sink.Byte(static_cast<uint8_t>(251 + n / 256));
                    sink.Byte(static_cast<uint8_t>(n));
                    return;
                }
                sink.Byte(28);
                sink.U16(static_cast<uint16_t>(value));
                return;
            }
            sink.Byte(255);
            sink.U32(static_cast<uint32_t>(fixed));
        }

        void Relative(Sink& sink, FixedPoint point, FixedPoint& current)
        {
            Operand(sink, static_cast<int64_t>(point.x) - current.x);
            Operand(sink, static_cast<int64_t>(point.y) - current.y);
            current = point;
        }

        uint32_t CoordinateCount(uint32_t verb)
        {
            switch (verb)
            {
            case NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE:
            case NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE:
                return 2;
            case NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD:
                return 4;
            case NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC:
                return 6;
            case NOTEPADS_MARKDOWN_SVG_COVERAGE_CLOSE:
                return 0;
            default:
                throw E_INVALIDARG;
            }
        }

        void ValidateAndPlan(SvgCoverageFontInput const& input, SvgFontAllocationContext& memory,
                             GlyphPlan& plan, bool& empty)
        {
            if (!input.path || reinterpret_cast<uintptr_t>(input.path) % alignof(MsSvgCoveragePath) ||
                sizeof(MsSvgCoveragePath) > UINTPTR_MAX - reinterpret_cast<uintptr_t>(input.path) ||
                input.request_index >= NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS || input.commands.size() > UINT32_MAX ||
                input.path->reserved || input.path->fill_rule != 0)
                throw E_INVALIDARG;
            if (!input.commands.empty() && (!input.commands.data() ||
                reinterpret_cast<uintptr_t>(input.commands.data()) % alignof(MsSvgCoverageCommand) ||
                Bytes(input.commands.size(), sizeof(MsSvgCoverageCommand)) >
                    UINTPTR_MAX - reinterpret_cast<uintptr_t>(input.commands.data())))
                throw E_INVALIDARG;
            auto const& path = *input.path;
            for (uint32_t bits : path.matrix_bits)
                if (!std::isfinite(std::bit_cast<float>(bits)))
                    throw E_INVALIDARG;
            double s = DecodeFloat(path.s_bits), t = DecodeFloat(path.t_bits);
            if ((path.s_bits & 0x7F800000) == 0x7F800000 || (path.t_bits & 0x7F800000) == 0x7F800000 ||
                s <= 0 || t <= 0 || s < t)
                throw E_INVALIDARG;
            bool moved = false, drawn = false;
            double min_x = Infinity, min_y = Infinity, max_x = -Infinity, max_y = -Infinity;
            for (size_t i = 0; i < input.commands.size(); ++i)
            {
                if (i % 256 == 0)
                    Work(memory, Bytes(std::min<size_t>(256, input.commands.size() - i), sizeof(MsSvgCoverageCommand)));
                auto const& command = input.commands[i];
                uint32_t count = CoordinateCount(command.verb);
                if (command.reserved || (command.verb != NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE && !moved))
                    throw E_INVALIDARG;
                for (uint32_t k = 0; k < 6; ++k)
                {
                    if (k >= count)
                    {
                        if (command.xy_bits[k])
                            throw E_INVALIDARG;
                    }
                    else
                    {
                        if ((command.xy_bits[k] & 0x7F800000) == 0x7F800000)
                            throw E_INVALIDARG;
                        double value = DecodeFloat(command.xy_bits[k]);
                        if (k % 2 == 0)
                        {
                            min_x = std::min(min_x, value);
                            max_x = std::max(max_x, value);
                        }
                        else
                        {
                            min_y = std::min(min_y, value);
                            max_y = std::max(max_y, value);
                        }
                    }
                }
                moved |= command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE;
                drawn |= command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE || command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD ||
                         command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC;
            }
            empty = !drawn || !path.width || !path.height;
            if (empty)
                return;
            auto baseline = [](double minimum, double maximum) -> float
            {
                // In this same-sign, factor-two domain, subtraction from the
                // center obeys Sterbenz and retains original f32 coordinates.
                // Across zero or a wider exponent range, zero is the exact
                // baseline; a needless rebase could lose a source mantissa bit.
                bool exact_subtraction = (minimum > 0 && minimum >= maximum / 2) ||
                                         (maximum < 0 && maximum <= minimum / 2);
                return exact_subtraction ? static_cast<float>(minimum / 2 + maximum / 2) : 0.0f;
            };
            plan.baseline_x = baseline(min_x, max_x);
            plan.baseline_y = baseline(min_y, max_y);
            double extent = std::max({Subtract(Interval(max_x), Interval(min_x)).hi,
                Subtract(Interval(max_y), Interval(min_y)).hi,
                Magnitude(Subtract(Interval(min_x), Interval(DecodeFloat(plan.baseline_x)))),
                Magnitude(Subtract(Interval(max_x), Interval(DecodeFloat(plan.baseline_x)))),
                Magnitude(Subtract(Interval(min_y), Interval(DecodeFloat(plan.baseline_y)))),
                Magnitude(Subtract(Interval(max_y), Interval(DecodeFloat(plan.baseline_y))))});
            // Largest power selected analytically from the whole-path extent;
            // no raster result, fixture identity, or scale search is involved.
            plan.exponent = extent > 0 ? 14 - std::ilogb(extent) : 0;
            plan.exponent = std::clamp(plan.exponent, -116, 137);
            // Signed-16 metadata has a stricter positive edge than the Type2
            // 16.16 operands. Account for that edge before encoding anything.
            if (std::ldexp(extent, plan.exponent) > 32767)
                --plan.exponent;
            plan.em_size = std::ldexp(1.0f, 11 - plan.exponent);
            if (!std::isfinite(plan.em_size) || plan.em_size <= 0)
                throw RepresentationDomain{CoordinateDomain};
            Interval q = Divide(Interval(s), Interval(t));
            auto absolute = [](uint32_t bits) { return Interval(std::abs(DecodeFloat(bits))); };
            plan.project_x = Multiply(q, Add(absolute(path.matrix_bits[0]), absolute(path.matrix_bits[1]))).hi;
            plan.project_y = Multiply(q, Add(absolute(path.matrix_bits[2]), absolute(path.matrix_bits[3]))).hi;
        }

        void CharString(SvgCoverageFontInput const& input, GlyphPlan const& plan, SvgFontAllocationContext& memory,
                        Sink& sink, Bounds* bounds)
        {
            FixedPoint current{}, start{};
            Point source_current{}, source_start{};
            bool closed = false;
            sink.Byte(139); // explicit width zero, nominalWidthX zero
            for (size_t i = 0; i < input.commands.size(); ++i)
            {
                if (i % 256 == 0)
                    Work(memory, Bytes(std::min<size_t>(256, input.commands.size() - i), sizeof(MsSvgCoverageCommand)));
                auto const& command = input.commands[i];
                if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_CLOSE)
                {
                    // Type2 implicit close leaves its current point at the last
                    // explicit endpoint. SVG Close restores the source start.
                    source_current = source_start;
                    closed = true;
                    continue;
                }
                if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE)
                {
                    source_current = source_start = SourcePoint(command, 0);
                    start = Encode(source_start, plan);
                    Relative(sink, start, current);
                    sink.Byte(21);
                    if (bounds)
                        bounds->Include(start);
                    closed = false;
                    continue;
                }
                if (closed)
                {
                    // A segment after Close starts a fresh closed-fill contour
                    // at the original subpath start. Repeated Close is a no-op.
                    Relative(sink, start, current);
                    sink.Byte(21);
                    closed = false;
                }
                FixedPoint previous = current;
                if (bounds)
                    bounds->Include(previous);
                if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE)
                {
                    source_current = SourcePoint(command, 0);
                    Relative(sink, Encode(source_current, plan), current);
                    sink.Byte(5);
                    if (bounds)
                        bounds->Include(current);
                }
                else
                {
                    Point c1, c2, end;
                    if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD)
                    {
                        Point control = SourcePoint(command, 0);
                        end = SourcePoint(command, 1);
                        c1 = QuadControl(source_current, control);
                        c2 = QuadControl(end, control);
                    }
                    else
                    {
                        c1 = SourcePoint(command, 0);
                        c2 = SourcePoint(command, 1);
                        end = SourcePoint(command, 2);
                    }
                    FixedPoint p1 = Encode(c1, plan), p2 = Encode(c2, plan), p3 = Encode(end, plan);
                    Relative(sink, p1, current);
                    Relative(sink, p2, current);
                    Relative(sink, p3, current);
                    sink.Byte(8);
                    if (bounds)
                    {
                        bounds->Include(p3);
                        CubicAxis(*bounds, previous, p1, p2, p3, true);
                        CubicAxis(*bounds, previous, p1, p2, p3, false);
                    }
                    source_current = end;
                }
            }
            sink.Byte(14);
        }

        void DictInteger(Sink& sink, int64_t value)
        {
            if (value < INT32_MIN || value > INT32_MAX)
                throw RepresentationDomain{FormatDomain};
            sink.Byte(29);
            sink.U32(static_cast<uint32_t>(value));
        }

        void MatrixElement(Sink& sink)
        {
            // CFF real 0.00048828125 = exactly 1/2048.
            constexpr uint8_t bytes[] = {30, 0x0A, 0x00, 0x04, 0x88, 0x28, 0x12, 0x5F};
            for (uint8_t byte : bytes)
                sink.Byte(byte);
        }

        struct CffPlan
        {
            uint32_t glyph_count{}, top_bytes{}, charset_offset{}, strings_offset{}, private_offset{}, bytes{};
            std::array<int16_t, 4> box{};
            int16_t min_lsb{}, min_rsb{}, max_extent{};
        };

        void TopDict(Sink& sink, CffPlan const& plan)
        {
            DictInteger(sink, 391); sink.Byte(2); // FullName
            DictInteger(sink, 391); sink.Byte(3); // FamilyName
            for (int16_t bound : plan.box)
                DictInteger(sink, bound);
            sink.Byte(5);
            MatrixElement(sink);
            DictInteger(sink, 0); DictInteger(sink, 0);
            MatrixElement(sink);
            DictInteger(sink, 0); DictInteger(sink, 0);
            sink.Byte(12); sink.Byte(7);
            DictInteger(sink, plan.charset_offset); sink.Byte(15);
            DictInteger(sink, plan.strings_offset); sink.Byte(17);
            DictInteger(sink, 4); DictInteger(sink, plan.private_offset); sink.Byte(18);
        }

        uint32_t Checked32(uint64_t value)
        {
            if (value > UINT32_MAX)
                throw RepresentationDomain{FormatDomain};
            return static_cast<uint32_t>(value);
        }

        uint32_t IndexBytes(uint32_t count, uint64_t payload)
        {
            return Checked32(count ? 3 + (static_cast<uint64_t>(count) + 1) * 4 + payload : 2);
        }

        CffPlan PlanCff(std::span<GlyphPlan const> glyphs)
        {
            CffPlan plan{};
            plan.glyph_count = static_cast<uint32_t>(glyphs.size()) + 1;
            plan.box = glyphs[0].box;
            plan.min_lsb = glyphs[0].box[0];
            int32_t min_rsb = -static_cast<int32_t>(glyphs[0].box[2]);
            plan.max_extent = glyphs[0].box[2];
            uint64_t char_bytes = 2; // .notdef: explicit width zero, endchar
            for (auto const& glyph : glyphs)
            {
                plan.box[0] = std::min(plan.box[0], glyph.box[0]);
                plan.box[1] = std::min(plan.box[1], glyph.box[1]);
                plan.box[2] = std::max(plan.box[2], glyph.box[2]);
                plan.box[3] = std::max(plan.box[3], glyph.box[3]);
                plan.min_lsb = std::min(plan.min_lsb, glyph.box[0]);
                min_rsb = std::min(min_rsb, -static_cast<int32_t>(glyph.box[2]));
                plan.max_extent = std::max(plan.max_extent, glyph.box[2]);
                char_bytes += glyph.bytes;
            }
            if (min_rsb < INT16_MIN || min_rsb > INT16_MAX)
                throw RepresentationDomain{FormatDomain};
            plan.min_rsb = static_cast<int16_t>(min_rsb);
            Sink top;
            TopDict(top, plan);
            plan.top_bytes = top.Position();
            uint64_t before_charset = 4 + IndexBytes(1, PostScriptName.size()) + IndexBytes(1, plan.top_bytes) +
                IndexBytes(plan.glyph_count, FamilyName.size() + 3ull * glyphs.size()) + 2;
            plan.charset_offset = Checked32(before_charset);
            plan.strings_offset = Checked32(before_charset + 1 + 2ull * glyphs.size());
            plan.private_offset = Checked32(static_cast<uint64_t>(plan.strings_offset) + IndexBytes(plan.glyph_count, char_bytes));
            plan.bytes = Checked32(static_cast<uint64_t>(plan.private_offset) + 4);
            if (plan.private_offset > INT32_MAX)
                throw RepresentationDomain{FormatDomain};
            return plan;
        }

        void IndexStart(Sink& sink, uint32_t count)
        {
            sink.U16(static_cast<uint16_t>(count));
            if (count)
                sink.Byte(4);
        }

        void Cff(Sink& sink, CffPlan const& plan, std::span<GlyphPlan const> glyphs,
                 std::span<SvgCoverageFontInput const> inputs, SvgFontAllocationContext& memory)
        {
            sink.Byte(1); sink.Byte(0); sink.Byte(4); sink.Byte(4);
            IndexStart(sink, 1);
            sink.U32(1); sink.U32(static_cast<uint32_t>(PostScriptName.size()) + 1);
            sink.Ascii(PostScriptName);
            IndexStart(sink, 1);
            sink.U32(1); sink.U32(plan.top_bytes + 1);
            uint32_t top_start = sink.Position();
            TopDict(sink, plan);
            if (sink.Position() - top_start != plan.top_bytes)
                throw E_UNEXPECTED;
            IndexStart(sink, plan.glyph_count);
            sink.U32(1);
            uint32_t offset = static_cast<uint32_t>(FamilyName.size()) + 1;
            sink.U32(offset);
            for (uint32_t i = 1; i < plan.glyph_count; ++i)
            {
                offset += 3;
                sink.U32(offset);
            }
            sink.Ascii(FamilyName);
            for (uint32_t i = 0; i < glyphs.size(); ++i)
                sink.Ascii(GlyphNames[i]);
            IndexStart(sink, 0); // GlobalSubrs
            if (sink.Position() != plan.charset_offset)
                throw E_UNEXPECTED;
            sink.Byte(0); // charset format 0, .notdef implicit
            for (uint32_t i = 0; i < glyphs.size(); ++i)
                sink.U16(static_cast<uint16_t>(392 + i));
            if (sink.Position() != plan.strings_offset)
                throw E_UNEXPECTED;
            IndexStart(sink, plan.glyph_count);
            sink.U32(1);
            offset = 3;
            sink.U32(offset);
            for (auto const& glyph : glyphs)
            {
                offset += glyph.bytes;
                sink.U32(offset);
            }
            sink.Byte(139); sink.Byte(14); // .notdef
            for (auto const& glyph : glyphs)
            {
                uint32_t start = sink.Position();
                CharString(inputs[glyph.input], glyph, memory, sink, nullptr);
                if (sink.Position() - start != glyph.bytes)
                    throw E_UNEXPECTED;
            }
            if (sink.Position() != plan.private_offset)
                throw E_UNEXPECTED;
            sink.Byte(139); sink.Byte(20); // defaultWidthX 0
            sink.Byte(139); sink.Byte(21); // nominalWidthX 0
            sink.Finish(plan.bytes);
        }

        constexpr std::string_view NameStrings[] = {FamilyName, "Regular", PostScriptName,
                                                     FamilyName, "Version 1.000", PostScriptName};

        uint32_t NameBytes() noexcept
        {
            uint32_t result = 6 + 12 * 6;
            for (auto value : NameStrings)
                result += static_cast<uint32_t>(value.size()) * 2;
            return result;
        }

        void Name(Sink& sink)
        {
            sink.U16(0); sink.U16(6); sink.U16(6 + 12 * 6);
            uint16_t offset{};
            for (uint16_t i = 0; i < 6; ++i)
            {
                sink.U16(3); sink.U16(1); sink.U16(0x0409); sink.U16(i + 1);
                uint16_t length = static_cast<uint16_t>(NameStrings[i].size() * 2);
                sink.U16(length); sink.U16(offset);
                offset = static_cast<uint16_t>(offset + length);
            }
            for (auto value : NameStrings)
                sink.Utf16(value);
        }

        int16_t Ascent(CffPlan const& plan) noexcept
        {
            return std::max<int16_t>(0, plan.box[3]);
        }

        int16_t Descent(CffPlan const& plan) noexcept
        {
            return std::min<int16_t>(0, plan.box[1]);
        }

        void Os2(Sink& sink, CffPlan const& plan)
        {
            sink.U16(4); sink.U16(0); sink.U16(400); sink.U16(5); sink.U16(0);
            sink.Zeros(22); // sub/superscript, strikeout and family class
            sink.Zeros(10); // PANOSE unspecified
            sink.U32(0); sink.U32(0x10000000); sink.U32(0); sink.U32(0); // BMP PUA
            sink.Ascii("MKST");
            sink.U16(0x0040); sink.U16(0xE000); sink.U16(static_cast<uint16_t>(0xE000 + plan.glyph_count - 2));
            sink.U16(static_cast<uint16_t>(Ascent(plan))); sink.U16(static_cast<uint16_t>(Descent(plan))); sink.U16(0);
            sink.U16(static_cast<uint16_t>(Ascent(plan))); sink.U16(static_cast<uint16_t>(-static_cast<int32_t>(Descent(plan))));
            sink.Zeros(8); // code page ranges
            sink.U16(0); sink.U16(0); sink.U16(0); sink.U16(0); sink.U16(1);
        }

        void Cmap(Sink& sink, CffPlan const& plan)
        {
            sink.U16(0); sink.U16(1);
            sink.U16(3); sink.U16(1); sink.U32(12);
            sink.U16(4); sink.U16(32); sink.U16(0); sink.U16(4);
            sink.U16(4); sink.U16(1); sink.U16(0);
            sink.U16(static_cast<uint16_t>(0xE000 + plan.glyph_count - 2)); sink.U16(0xFFFF);
            sink.U16(0);
            sink.U16(0xE000); sink.U16(0xFFFF);
            sink.U16(static_cast<uint16_t>(1 - 0xE000)); sink.U16(1);
            sink.U16(0); sink.U16(0);
        }

        void Head(Sink& sink, CffPlan const& plan)
        {
            sink.U32(0x00010000); sink.U32(0x00010000); sink.U32(0); sink.U32(0x5F0F3CF5);
            sink.U16(1); sink.U16(UnitsPerEm);
            // Fixed 2026-11-02 UTC in the 1904 epoch, never wall-clock state.
            sink.U32(0); sink.U32(3876422400u); sink.U32(0); sink.U32(3876422400u);
            for (int16_t bound : plan.box)
                sink.U16(static_cast<uint16_t>(bound));
            sink.U16(0); sink.U16(1); sink.U16(2); sink.U16(0); sink.U16(0);
        }

        void Hhea(Sink& sink, CffPlan const& plan)
        {
            sink.U32(0x00010000);
            sink.U16(static_cast<uint16_t>(Ascent(plan))); sink.U16(static_cast<uint16_t>(Descent(plan))); sink.U16(0);
            sink.U16(0); // all advance widths are zero
            sink.U16(static_cast<uint16_t>(plan.min_lsb)); sink.U16(static_cast<uint16_t>(plan.min_rsb));
            sink.U16(static_cast<uint16_t>(plan.max_extent));
            sink.U16(1); sink.U16(0); sink.U16(0); // vertical caret
            sink.Zeros(8); sink.U16(0); sink.U16(static_cast<uint16_t>(plan.glyph_count));
        }

        struct Table
        {
            uint32_t tag, offset{}, bytes{};
        };

        uint32_t Read32(uint8_t const* value) noexcept
        {
            return static_cast<uint32_t>(value[0]) << 24 | static_cast<uint32_t>(value[1]) << 16 |
                   static_cast<uint32_t>(value[2]) << 8 | value[3];
        }

        uint32_t Checksum(SvgFontAllocationContext& memory, std::span<uint8_t const> bytes, bool head = false)
        {
            uint32_t sum{};
            for (size_t i = 0; i < bytes.size(); i += 4)
            {
                if (i % 4096 == 0)
                    Work(memory, std::min<size_t>(4096, bytes.size() - i));
                uint32_t word{};
                for (size_t k = 0; k < 4 && i + k < bytes.size(); ++k)
                    word |= static_cast<uint32_t>(bytes[i + k]) << (24 - 8 * k);
                if (!head || i != 8)
                    sum += word;
            }
            return sum;
        }

        void Patch32(SvgFontAllocationContext& memory, SvgFontVector<uint8_t>& bytes, uint32_t offset, uint32_t value)
        {
            if (offset > bytes.size() || bytes.size() - offset < 4)
                throw E_UNEXPECTED;
            Sink sink(memory, bytes.data() + offset, 4);
            sink.U32(value);
        }

        void BuildBytes(std::span<GlyphPlan const> glyphs, std::span<SvgCoverageFontInput const> inputs,
                        SvgFontAllocationContext& memory, SvgFontVector<uint8_t>& bytes)
        {
            CffPlan plan = PlanCff(glyphs);
            std::array<Table, 9> tables{{{0x43464620, 0, plan.bytes}, {0x4F532F32, 0, 96}, {0x636D6170, 0, 44},
                {0x68656164, 0, 54}, {0x68686561, 0, 36}, {0x686D7478, 0, 4 * plan.glyph_count},
                {0x6D617870, 0, 6}, {0x6E616D65, 0, NameBytes()}, {0x706F7374, 0, 32}}};
            uint64_t total = 12 + 16 * tables.size();
            for (auto& table : tables)
            {
                table.offset = Checked32(total);
                total += (static_cast<uint64_t>(table.bytes) + 3) & ~3ull;
            }
            uint32_t size = Checked32(total);
            if (size > SIZE_MAX)
                throw E_OUTOFMEMORY;
            Work(memory, 0);
            bytes.reserve(size);
            // The single final allocation is admitted before initialization;
            // every physical initialization window is paid before touching it.
            while (bytes.size() < size)
            {
                size_t next = bytes.size() + std::min<size_t>(4096, static_cast<size_t>(size) - bytes.size());
                Work(memory, next - bytes.size());
                bytes.resize(next);
            }
            {
                Sink directory(memory, bytes.data(), 12 + 16 * tables.size());
                directory.U32(0x4F54544F); directory.U16(9); directory.U16(128); directory.U16(3); directory.U16(16);
                for (auto const& table : tables)
                {
                    directory.U32(table.tag); directory.U32(0); directory.U32(table.offset); directory.U32(table.bytes);
                }
                directory.Finish(12 + 16 * static_cast<uint32_t>(tables.size()));
            }
            for (uint32_t i = 0; i < tables.size(); ++i)
            {
                auto const& table = tables[i];
                Sink sink(memory, bytes.data() + table.offset, table.bytes);
                switch (i)
                {
                case 0:
                    Cff(sink, plan, glyphs, inputs, memory);
                    break;
                case 1:
                    Os2(sink, plan);
                    break;
                case 2:
                    Cmap(sink, plan);
                    break;
                case 3:
                    Head(sink, plan);
                    break;
                case 4:
                    Hhea(sink, plan);
                    break;
                case 5:
                    sink.U16(0); sink.U16(0); // empty .notdef metrics
                    for (auto const& glyph : glyphs)
                    {
                        sink.U16(0); sink.U16(static_cast<uint16_t>(glyph.box[0]));
                    }
                    break;
                case 6:
                    sink.U32(0x00005000); sink.U16(static_cast<uint16_t>(plan.glyph_count));
                    break;
                case 7:
                    Name(sink);
                    break;
                case 8:
                    sink.U32(0x00030000); sink.Zeros(28);
                    break;
                default:
                    throw E_UNEXPECTED;
                }
                sink.Finish(table.bytes);
                uint32_t check = Checksum(memory, {bytes.data() + table.offset, table.bytes});
                Patch32(memory, bytes, 12 + 16 * i + 4, check);
            }
            uint32_t sum = Checksum(memory, bytes);
            Patch32(memory, bytes, tables[3].offset + 8, 0xB1B0AFBA - sum);
            // Independent reads of the emitted directory and all table bytes.
            // Arithmetic/layout defects are real writer failures, not domains.
            Work(memory, 12 + 16 * tables.size());
            if (Read32(bytes.data()) != 0x4F54544F)
                throw E_UNEXPECTED;
            for (uint32_t i = 0; i < tables.size(); ++i)
            {
                auto const* record = bytes.data() + 12 + 16 * i;
                auto const& table = tables[i];
                if (Read32(record) != table.tag || Read32(record + 8) != table.offset ||
                    Read32(record + 12) != table.bytes || table.offset % 4 ||
                    static_cast<uint64_t>(table.offset) + table.bytes > bytes.size())
                    throw E_UNEXPECTED;
                if (Checksum(memory, {bytes.data() + table.offset, table.bytes}, i == 3) != Read32(record + 4))
                    throw E_UNEXPECTED;
            }
            if (Checksum(memory, bytes) != 0xB1B0AFBA)
                throw E_UNEXPECTED;
            Work(memory, 0);
        }
    }

    void BuildSvgCoverageFont(std::span<SvgCoverageFontInput const> inputs,
                              SvgFontAllocationContext& memory, SvgCoverageFont& output)
    {
        Work(memory, 0);
        if (std::fegetround() != FE_TONEAREST)
            throw E_UNEXPECTED;
        if (inputs.size() > NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS || (!inputs.empty() && (!inputs.data() ||
            reinterpret_cast<uintptr_t>(inputs.data()) % alignof(SvgCoverageFontInput) ||
            Bytes(inputs.size(), sizeof(SvgCoverageFontInput)) > UINTPTR_MAX - reinterpret_cast<uintptr_t>(inputs.data()))) ||
            output.bytes.get_allocator().context != &memory || output.bytes.get_allocator().category != Font ||
            !output.bytes.empty() || output.encoded_mask || output.empty_mask || output.ineligible_mask)
            throw E_INVALIDARG;
        SvgCoverageFont staged(&memory);
        std::array<GlyphPlan, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> plans{};
        uint32_t count{}, seen{};
        for (uint32_t i = 0; i < inputs.size(); ++i)
        {
            auto const& input = inputs[i];
            if (input.request_index >= NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS)
                throw E_INVALIDARG;
            uint32_t bit = uint32_t{1} << input.request_index;
            if (seen & bit)
                throw E_INVALIDARG;
            seen |= bit;
            GlyphPlan plan{};
            plan.input = i;
            bool empty{};
            try
            {
                ValidateAndPlan(input, memory, plan, empty);
                if (empty)
                {
                    staged.empty_mask |= bit;
                    continue;
                }
                Bounds bounds;
                Sink count_sink;
                CharString(input, plan, memory, count_sink, &bounds);
                plan.bytes = count_sink.Position();
                if (plan.bytes > MaximumCharStringBytes)
                    throw RepresentationDomain{FormatDomain};
                if (!bounds.present)
                    throw E_UNEXPECTED;
                plan.box = {BoundInteger(bounds.min_x, false), BoundInteger(bounds.min_y, false),
                            BoundInteger(bounds.max_x, true), BoundInteger(bounds.max_y, true)};
                if (-static_cast<int32_t>(plan.box[2]) < INT16_MIN || -static_cast<int32_t>(plan.box[2]) > INT16_MAX)
                    throw RepresentationDomain{FormatDomain};
                plan.gid = static_cast<uint16_t>(count + 1);
                plans[count++] = plan;
                staged.glyphs[input.request_index] = {plan.baseline_x, plan.baseline_y, plan.em_size, plan.gid, 0};
                staged.encoded_mask |= bit;
            }
            catch (RepresentationDomain const& domain)
            {
                staged.ineligible_mask |= bit;
                staged.reasons[input.request_index] = domain.reason;
            }
        }
        if (count)
        {
            // The representability pass is complete before allocating a font.
            // A collective UINT32/DICT offset limit is also a format domain;
            // actual reserve/admit/OOM/work failures retain their HRESULT.
            try
            {
                BuildBytes({plans.data(), count}, inputs, memory, staged.bytes);
            }
            catch (RepresentationDomain const& domain)
            {
                if (!staged.bytes.empty())
                    throw E_UNEXPECTED; // a second-pass disagreement is a writer bug
                for (auto const& plan : std::span<GlyphPlan const>(plans.data(), count))
                {
                    uint32_t request = inputs[plan.input].request_index;
                    staged.reasons[request] = domain.reason;
                    staged.glyphs[request] = {};
                }
                staged.ineligible_mask |= staged.encoded_mask;
                staged.encoded_mask = 0;
            }
        }
        if ((staged.encoded_mask | staged.empty_mask | staged.ineligible_mask) != seen ||
            (staged.encoded_mask & staged.empty_mask) || (staged.encoded_mask & staged.ineligible_mask) ||
            (staged.empty_mask & staged.ineligible_mask))
            throw E_UNEXPECTED;
        Work(memory, 0);
        output.bytes.swap(staged.bytes);
        output.glyphs = staged.glyphs;
        output.reasons = staged.reasons;
        output.encoded_mask = staged.encoded_mask;
        output.empty_mask = staged.empty_mask;
        output.ineligible_mask = staged.ineligible_mask;
    }
}

#pragma float_control(pop)
