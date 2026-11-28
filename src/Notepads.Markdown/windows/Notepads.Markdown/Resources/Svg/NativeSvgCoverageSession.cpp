// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "Resources/Svg/NativeSvgCoverageSession.h"
#include "Resources/Svg/NativeSvgCoverageFont.h"
#include "Resources/Svg/NativeSvgCoverageSampling.h"
#include <d2d1.h>
#include <dwrite_3.h>
#include <wincodec.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <bit>
#include <cmath>
#include <cstring>
#include <limits>
#include <span>
#include <utility>

#pragma float_control(precise, on, push)

namespace Notepads::Markdown::Native
{
    using SvgFontMemory::Closed;
    using SvgFontMemory::Quota;
    using SvgFontMemory::Font;
    using SvgFontMemory::Geometry;
    using SvgFontMemory::Scratch;
    using SvgFontMemory::Check;
    using SvgFontMemory::Bytes;
    using SvgFontMemory::RecordFault;
    using SvgFontMemory::Free;
    namespace
    {
        constexpr uint32_t SourceSide = 256, GroupSide = 256, BlockSide = 16;
        constexpr double TransformTolerance = 1.0 / 4096;
        struct CleanupStopped
        {
        };

        template <class T> void Drop(T *&object) noexcept
        {
            if (object)
            {
                auto value = std::exchange(object, nullptr);
                value->Release();
            }
        }

        uint64_t Add(uint64_t a, uint64_t b)
        {
            if (b > UINT64_MAX - a)
                throw E_INVALIDARG;
            return a + b;
        }

        struct AddressRange
        {
            uintptr_t begin{}, end{};
        };

        AddressRange Range(void const *pointer, uint64_t bytes, size_t alignment = 1)
        {
            auto address = reinterpret_cast<uintptr_t>(pointer);
            if (bytes && (!pointer || address % alignment || bytes > UINTPTR_MAX - address))
                throw E_INVALIDARG;
            return {address, bytes ? address + static_cast<uintptr_t>(bytes) : address};
        }

        bool Overlap(AddressRange a, AddressRange b) noexcept
        {
            return a.begin < a.end && b.begin < b.end && a.begin < b.end && b.begin < a.end;
        }

        void Disjoint(AddressRange a, AddressRange b)
        {
            if (Overlap(a, b))
                throw E_INVALIDARG;
        }

        float Value(uint32_t bits) noexcept
        {
            return std::bit_cast<float>(bits);
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

        void Work(SvgFontAllocationContext &memory, uint64_t amount)
        {
            do
            {
                uint64_t window = std::min<uint64_t>(4096, amount);
                Check(memory.services.checkpoint(memory.services.context, window));
                amount -= window;
            } while (amount);
        }

        void PixelWork(void *context, uint64_t amount)
        {
            Work(*static_cast<SvgFontAllocationContext *>(context), amount);
        }

        void ValidateBatch(MsSvgCoverageBatch const *batch, MsSvgCoverageResult *result,
                           SvgFontAllocationContext &memory)
        {
            auto batch_range = Range(batch, sizeof(*batch), alignof(MsSvgCoverageBatch));
            auto result_range = Range(result, sizeof(*result), alignof(MsSvgCoverageResult));
            Disjoint(batch_range, result_range);
            if (batch->size != sizeof(*batch) || batch->version != NOTEPADS_MARKDOWN_SVG_COVERAGE_ABI ||
                batch->path_count > NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS || result->size != sizeof(*result) ||
                result->version != NOTEPADS_MARKDOWN_SVG_COVERAGE_ABI || result->ready_mask || result->ineligible_mask ||
                result->completed || result->reserved)
                throw E_INVALIDARG;
            for (auto reason : result->reasons)
                if (reason)
                    throw E_INVALIDARG;
#if INTPTR_MAX == INT32_MAX
            if (batch->paths_padding || batch->commands_padding || batch->a8_padding)
                throw E_INVALIDARG;
#endif
            auto paths =
                Range(batch->paths, Bytes(batch->path_count, sizeof(*batch->paths)), alignof(MsSvgCoveragePath));
            auto commands = Range(batch->commands, Bytes(batch->command_count, sizeof(*batch->commands)),
                                  alignof(MsSvgCoverageCommand));
            auto arena = Range(batch->a8, batch->a8_capacity);
            Disjoint(result_range, paths);
            Disjoint(result_range, commands);
            for (uint32_t i = 0; i < batch->path_count; ++i)
            {
                Work(memory, sizeof(MsSvgCoveragePath) + 32);
                auto const &path = batch->paths[i];
                if (path.reserved || path.fill_rule > 1 || path.target_x || path.target_y ||
                    path.command_start > batch->command_count ||
                    path.command_count > batch->command_count - path.command_start)
                    throw E_INVALIDARG;
                for (auto bits : path.matrix_bits)
                    if (!std::isfinite(Value(bits)))
                        throw E_INVALIDARG;
                float s = Value(path.s_bits), t = Value(path.t_bits);
                if (!std::isfinite(s) || !std::isfinite(t) || !(t > 0) || !(s >= t) ||
                    (i && (path.s_bits != batch->paths[0].s_bits || path.t_bits != batch->paths[0].t_bits)))
                    throw E_INVALIDARG;
                uint64_t needed = 0;
                if (path.width && path.height)
                {
                    if (path.stride < path.width)
                        throw E_INVALIDARG;
                    needed = Add(Bytes(path.stride, uint64_t{path.height} - 1), path.width);
                }
                if (needed > path.length || path.a8_offset > batch->a8_capacity ||
                    path.length > batch->a8_capacity - path.a8_offset)
                    throw E_INVALIDARG;
                // Range(arena) already proved the base plus every offset fits uintptr_t.
                AddressRange output{arena.begin + static_cast<uintptr_t>(path.a8_offset),
                                    arena.begin + static_cast<uintptr_t>(path.a8_offset + path.length)};
                Disjoint(output, result_range);
                Disjoint(output, batch_range);
                Disjoint(output, paths);
                Disjoint(output, commands);
                for (uint32_t j = 0; j < i; ++j)
                {
                    auto const &previous = batch->paths[j];
                    Disjoint(output, {arena.begin + static_cast<uintptr_t>(previous.a8_offset),
                                      arena.begin + static_cast<uintptr_t>(previous.a8_offset + previous.length)});
                }
            }
        }

        struct Retirement
        {
            bool freed{};
        };

        // Only our sentinel is permitted to destroy this object. Legal late
        // external Release calls stop at that sentinel and touch atomics only.
        class ByteOwner final : public IUnknown
        {
          public:
            ByteOwner(SvgFontAllocationContext &memory, Retirement &receipt, SvgFontVector<uint8_t> &bytes) noexcept
                : memory_(&memory), receipt_(&receipt), worker_(GetCurrentThreadId()), bytes_(std::move(bytes))
            {
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown))
                    return E_NOINTERFACE;
                *output = static_cast<IUnknown *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return references_.fetch_add(1, std::memory_order_relaxed) + 1;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                ULONG current = references_.load(std::memory_order_acquire);
                for (;;)
                {
                    if (current == 1 && GetCurrentThreadId() != worker_)
                        return 1;
                    if (references_.compare_exchange_weak(current, current - 1, std::memory_order_acq_rel))
                        break;
                }
                if (current != 1)
                    return current - 1;
                auto memory = memory_;
                auto receipt = receipt_;
                this->~ByteOwner();
                SvgFontAllocator<ByteOwner>(memory, Font).deallocate(this, 1);
                receipt->freed = true;
                return 0;
            }
            ULONG References() const noexcept
            {
                return references_.load(std::memory_order_acquire);
            }
            std::span<uint8_t const> BytesView() const noexcept
            {
                return bytes_;
            }

          private:
            std::atomic<ULONG> references_{1};
            SvgFontAllocationContext *memory_;
            Retirement *receipt_;
            DWORD worker_;
            SvgFontVector<uint8_t> bytes_;
        };

        class OutlineSink final : public ID2D1SimplifiedGeometrySink
        {
          public:
            OutlineSink(SvgFontAllocationContext &memory, Retirement &receipt) noexcept
                : memory_(&memory), receipt_(&receipt), worker_(GetCurrentThreadId()),
                  commands_(SvgFontAllocator<MsSvgCoverageCommand>(&memory, Geometry))
            {
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(ID2D1SimplifiedGeometrySink))
                    return E_NOINTERFACE;
                *output = static_cast<ID2D1SimplifiedGeometrySink *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return references_.fetch_add(1, std::memory_order_relaxed) + 1;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                ULONG current = references_.load(std::memory_order_acquire);
                for (;;)
                {
                    if (current == 1 && GetCurrentThreadId() != worker_)
                        return 1;
                    if (references_.compare_exchange_weak(current, current - 1, std::memory_order_acq_rel))
                        break;
                }
                if (current != 1)
                    return current - 1;
                auto memory = memory_;
                auto receipt = receipt_;
                this->~OutlineSink();
                SvgFontAllocator<OutlineSink>(memory, Geometry).deallocate(this, 1);
                receipt->freed = true;
                return 0;
            }
            ULONG References() const noexcept
            {
                return references_.load(std::memory_order_acquire);
            }
            std::span<MsSvgCoverageCommand const> Commands() const noexcept
            {
                return commands_;
            }
            void STDMETHODCALLTYPE SetFillMode(D2D1_FILL_MODE mode) noexcept override
            {
                if (mode != D2D1_FILL_MODE_ALTERNATE && mode != D2D1_FILL_MODE_WINDING)
                    Fault(E_UNEXPECTED);
            }
            void STDMETHODCALLTYPE SetSegmentFlags(D2D1_PATH_SEGMENT) noexcept override
            {
                // Stroke-only flags have no effect on this whole-fill glyph.
            }
            void STDMETHODCALLTYPE BeginFigure(D2D1_POINT_2F point, D2D1_FIGURE_BEGIN begin) noexcept override
            {
                if (figure_ || closed_ || (begin != D2D1_FIGURE_BEGIN_FILLED && begin != D2D1_FIGURE_BEGIN_HOLLOW))
                {
                    Fault(E_UNEXPECTED);
                    return;
                }
                figure_ = true;
                filled_ = begin == D2D1_FIGURE_BEGIN_FILLED;
                figure_start_ = commands_.size();
                Append(NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE, {point.x, point.y});
            }
            void STDMETHODCALLTYPE AddLines(D2D1_POINT_2F const *points, UINT32 count) noexcept override
            {
                if (!figure_ || (count && !points))
                {
                    Fault(E_UNEXPECTED);
                    return;
                }
                for (UINT32 i = 0; i < count && fault_ == S_OK; ++i)
                    Append(NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE, {points[i].x, points[i].y});
            }
            void STDMETHODCALLTYPE AddBeziers(D2D1_BEZIER_SEGMENT const *curves, UINT32 count) noexcept override
            {
                if (!figure_ || (count && !curves))
                {
                    Fault(E_UNEXPECTED);
                    return;
                }
                for (UINT32 i = 0; i < count && fault_ == S_OK; ++i)
                {
                    auto const &curve = curves[i];
                    Append(NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC, {curve.point1.x, curve.point1.y, curve.point2.x, curve.point2.y,
                                                   curve.point3.x, curve.point3.y});
                }
            }
            void STDMETHODCALLTYPE EndFigure(D2D1_FIGURE_END end) noexcept override
            {
                if (!figure_ || (end != D2D1_FIGURE_END_OPEN && end != D2D1_FIGURE_END_CLOSED))
                {
                    Fault(E_UNEXPECTED);
                    return;
                }
                // Open filled figures close implicitly, just as in the input.
                Append(NOTEPADS_MARKDOWN_SVG_COVERAGE_CLOSE, {});
                // Outline may emit hollow remnants for cancelled/zero-area
                // edges. They have no fill, including with winding mode.
                if (!filled_)
                    commands_.resize(figure_start_);
                figure_ = false;
            }
            HRESULT STDMETHODCALLTYPE Close() noexcept override
            {
                if (figure_)
                    Fault(E_UNEXPECTED);
                closed_ = true;
                return fault_;
            }

          private:
            void Fault(HRESULT hr) noexcept
            {
                if (fault_ == S_OK)
                {
                    fault_ = hr;
                    RecordFault(memory_->services.context, hr);
                }
            }
            void Append(uint32_t verb, std::initializer_list<float> values) noexcept
            {
                if (fault_ != S_OK)
                    return;
                try
                {
                    Work(*memory_, sizeof(MsSvgCoverageCommand));
                    MsSvgCoverageCommand command{};
                    command.verb = verb;
                    size_t index = 0;
                    for (float value : values)
                    {
                        if (!std::isfinite(value))
                            throw E_UNEXPECTED;
                        command.xy_bits[index++] = std::bit_cast<uint32_t>(value);
                    }
                    if (commands_.size() == UINT32_MAX)
                        throw Quota;
                    if (commands_.size() == commands_.capacity())
                    {
                        size_t capacity = commands_.capacity();
                        size_t next = capacity ? std::min<size_t>(UINT32_MAX, capacity + capacity / 2 + 1) : 32;
                        Work(*memory_, Bytes(commands_.size(), sizeof(command)));
                        commands_.reserve(next);
                    }
                    commands_.push_back(command);
                }
                catch (HRESULT hr)
                {
                    Fault(hr);
                }
                catch (std::bad_alloc const &)
                {
                    Fault(E_OUTOFMEMORY);
                }
                catch (...)
                {
                    Fault(E_UNEXPECTED);
                }
            }
            std::atomic<ULONG> references_{1};
            SvgFontAllocationContext *memory_;
            Retirement *receipt_;
            DWORD worker_;
            SvgFontVector<MsSvgCoverageCommand> commands_;
            HRESULT fault_{S_OK};
            size_t figure_start_{};
            bool figure_{}, filled_{}, closed_{};
        };

        double IntegerShiftRounding(double base, uint32_t extent, double uncertainty = 0) noexcept
        {
            if (!extent && !uncertainty)
                return std::abs(double{static_cast<float>(base)} - base);
            double maximum = std::max(std::abs(base), std::abs(base - extent)) + uncertainty;
            float value = static_cast<float>(maximum);
            if (!std::isfinite(value))
                return std::numeric_limits<double>::infinity();
            double spacing = double{std::nextafter(value, std::numeric_limits<float>::infinity())} - value;
            if (!uncertainty && spacing <= 1 && std::fmod(base, spacing) == 0)
                return 0;
            return spacing / 2;
        }

        // Bound affine rounding in the admitted complete source domain.
        double FloatAffineError(float a, float b, float translation, float x, float y, uint32_t extent) noexcept
        {
            double ax = double{a} * x, by = double{b} * y;
            float ax_float = static_cast<float>(ax), by_float = static_cast<float>(by);
            if (!std::isfinite(ax_float) || !std::isfinite(by_float))
                return std::numeric_limits<double>::infinity();
            double products = std::abs(double{ax_float} - ax) + std::abs(double{by_float} - by);
            double shift = IntegerShiftRounding(translation, extent);
            double sum = double{ax_float} + by_float;
            float rounded_sum = static_cast<float>(sum);
            if (!std::isfinite(rounded_sum))
                return std::numeric_limits<double>::infinity();
            double first = std::abs(double{rounded_sum} - sum) +
                           IntegerShiftRounding(double{rounded_sum} + translation, extent, shift);
            double left = IntegerShiftRounding(double{ax_float} + translation, extent, shift);
            double second = left + IntegerShiftRounding(sum + translation, extent, shift + left);
            double right = IntegerShiftRounding(double{by_float} + translation, extent, shift);
            double third = right + IntegerShiftRounding(sum + translation, extent, shift + right);
            return products + shift +
                   std::max({first, second, third, IntegerShiftRounding(ax + by + translation, extent)});
        }
    }

    struct SvgCoverageSession::State
    {
        explicit State(SvgFontAllocationContext &value) noexcept
            : memory(value), font(&value), sums(SvgFontAllocator<SvgCoverageWide>(&value, Scratch)),
              tile(SvgFontAllocator<uint8_t>(&value, Scratch))
        {
        }

        SvgFontAllocationContext &memory;
        SvgCoverageFont font;
        SvgFontVector<SvgCoverageWide> sums;
        SvgFontVector<uint8_t> tile;
        std::array<MsSvgCoveragePath, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> paths{};
        std::array<SvgCoverageSampling, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> plans{};
        std::array<std::array<float, 6>, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> matrices{};
        std::array<OutlineSink *, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> sinks{};
        std::array<Retirement, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> sink_receipts{};
        ByteOwner *owner{};
        Retirement owner_receipt{};
        ID2D1Factory *d2d{};
        ID2D1PathGeometry *geometry{};
        ID2D1GeometrySink *geometry_sink{};
        IWICImagingFactory *wic{};
        IWICBitmap *bitmap{};
        IWICBitmapLock *bitmap_lock{};
        ID2D1RenderTarget *target{};
        ID2D1SolidColorBrush *brush{};
        IDWriteFactory5 *factory{};
        IDWriteInMemoryFontFileLoader *loader{};
        IDWriteFontFile *file{};
        IDWriteFontFace *face{};
        IDWriteRenderingParams *rendering{};
        IDWriteRenderingParams *actual_rendering{};
        IDWriteRenderingParams1 *actual_rendering1{};
        IDWriteRenderingParams2 *actual_rendering2{};
        IDWriteRenderingParams3 *actual_rendering3{};
        notepads_markdown_svg_work_token pixel_lease{};
        uint64_t pixel_reserved{}, pixel_admitted{};
        uint32_t maximum_bitmap_side{};
        bool pixels_reserved{}, registered{}, drawing{}, dependency{}, quarantine{};
        HRESULT cleanup{S_OK};

        void Api(HRESULT hr)
        {
            Check(hr);
            Work(memory, 0);
        }

        void CleanupError(HRESULT hr) noexcept
        {
            if (hr == S_OK)
                return;
            if (cleanup == S_OK)
                cleanup = hr < 0 ? hr : E_UNEXPECTED;
            RecordFault(memory.services.context, hr);
        }

        void EnsureD2D()
        {
            if (!d2d)
            {
                Work(memory, 0);
                Api(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &d2d));
            }
        }

        bool PrepareMatrix(uint32_t index, std::span<MsSvgCoverageCommand const> commands)
        {
            auto const &path = paths[index];
            auto const &plan = plans[index];
            double q = static_cast<double>(plan.numerator) / static_cast<double>(plan.denominator);
            double determinant = double{Value(path.matrix_bits[0])} * Value(path.matrix_bits[3]) -
                                 double{Value(path.matrix_bits[1])} * Value(path.matrix_bits[2]);
            if (determinant == 0)
                return false;
            std::array<double, 6> exact{}, error{};
            for (size_t i = 0; i < 6; ++i)
            {
                exact[i] = q * Value(path.matrix_bits[i]);
                float value = static_cast<float>(exact[i]);
                if (!std::isfinite(value))
                    return false;
                matrices[index][i] = value;
                // Two outward double ULPs cover rational division and product.
                double rounding =
                    std::abs(std::nextafter(exact[i], std::numeric_limits<double>::infinity()) - exact[i]);
                error[i] = std::abs(double{value} - exact[i]) + 4 * rounding;
            }
            auto const &matrix = matrices[index];
            for (size_t i = 0; i < commands.size(); ++i)
            {
                if (i % 128 == 0)
                    Work(memory, Bytes(std::min<size_t>(128, commands.size() - i), sizeof(commands[0])));
                auto const &command = commands[i];
                for (uint32_t j = 0, count = CoordinateCount(command.verb); j < count; j += 2)
                {
                    float x = Value(command.xy_bits[j]), y = Value(command.xy_bits[j + 1]);
                    double ex = error[0] * std::abs(double{x}) + error[2] * std::abs(double{y}) + error[4];
                    double ey = error[1] * std::abs(double{x}) + error[3] * std::abs(double{y}) + error[5];
                    // The one actual glyph realization uses global q*M only.
                    // CopyPixels never changes its geometry transform.
                    ex += FloatAffineError(matrix[0], matrix[2], matrix[4], x, y, 0);
                    ey += FloatAffineError(matrix[1], matrix[3], matrix[5], x, y, 0);
                    if (!std::isfinite(ex) || !std::isfinite(ey) || ex > TransformTolerance || ey > TransformTolerance)
                        return false;
                }
            }
            return true;
        }

        bool Normalize(uint32_t index, std::span<MsSvgCoverageCommand const> commands)
        {
            auto const &plan = plans[index];
            auto const &path = paths[index];
            double squared = 0;
            for (uint32_t i = 0; i < 4; ++i)
            {
                double value = Value(path.matrix_bits[i]);
                squared += value * value;
            }
            // Frobenius is an upper bound for the required operator norm.
            double norm = std::nextafter(
                std::sqrt(squared) * (static_cast<double>(plan.numerator) / static_cast<double>(plan.denominator)) *
                    (1 + 16 * std::numeric_limits<double>::epsilon()),
                std::numeric_limits<double>::infinity());
            double limit = TransformTolerance / norm;
            float tolerance = static_cast<float>(limit);
            if (double{tolerance} > limit)
                tolerance = std::nextafter(tolerance, 0.0f);
            if (!(tolerance > 0) || !std::isfinite(tolerance))
                return false;
            EnsureD2D();
            Work(memory, 0);
            Api(d2d->CreatePathGeometry(&geometry));
            Work(memory, 0);
            Api(geometry->Open(&geometry_sink));
            geometry_sink->SetFillMode(D2D1_FILL_MODE_ALTERNATE);
            bool figure = false;
            D2D1_POINT_2F start{};
            for (size_t i = 0; i < commands.size(); ++i)
            {
                if (i % 128 == 0)
                    Work(memory, Bytes(std::min<size_t>(128, commands.size() - i), sizeof(commands[0])));
                auto const &command = commands[i];
                auto point = [&command](uint32_t at)
                { return D2D1::Point2F(Value(command.xy_bits[at]), Value(command.xy_bits[at + 1])); };
                if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE)
                {
                    if (figure)
                        geometry_sink->EndFigure(D2D1_FIGURE_END_OPEN);
                    start = point(0);
                    geometry_sink->BeginFigure(start, D2D1_FIGURE_BEGIN_FILLED);
                    figure = true;
                }
                else if (command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_CLOSE)
                {
                    if (figure)
                        geometry_sink->EndFigure(D2D1_FIGURE_END_CLOSED);
                    figure = false;
                }
                else
                {
                    if (!figure)
                    {
                        geometry_sink->BeginFigure(start, D2D1_FIGURE_BEGIN_FILLED);
                        figure = true;
                    }
                    switch (command.verb)
                    {
                    case NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE:
                        geometry_sink->AddLine(point(0));
                        break;
                    case NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD:
                        geometry_sink->AddQuadraticBezier(D2D1::QuadraticBezierSegment(point(0), point(2)));
                        break;
                    case NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC:
                        geometry_sink->AddBezier(D2D1::BezierSegment(point(0), point(2), point(4)));
                        break;
                    }
                }
            }
            if (figure)
                geometry_sink->EndFigure(D2D1_FIGURE_END_OPEN);
            Work(memory, 0);
            Api(geometry_sink->Close());
            Drop(geometry_sink);
            auto allocated = SvgFontAllocator<OutlineSink>(&memory, Geometry).allocate(1);
            sinks[index] = new (allocated) OutlineSink(memory, sink_receipts[index]);
            Work(memory, 0);
            // Outline stays in local space. Applying q*M here would apply it twice.
            Api(geometry->Outline(nullptr, tolerance, sinks[index]));
            Api(sinks[index]->Close());
            Drop(geometry);
            paths[index].fill_rule = 0;
            return true;
        }

        void LoadFont(SvgCoverageStatistics &statistics)
        {
            Work(memory, 0);
            Api(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory5),
                                    reinterpret_cast<IUnknown **>(&factory)));
            Work(memory, 0);
            Api(factory->CreateInMemoryFontFileLoader(&loader));
            Work(memory, 0);
            HRESULT hr = factory->RegisterFontFileLoader(loader);
            if (hr == S_OK)
                registered = true;
            Api(hr);
            if (font.bytes.empty() || font.bytes.size() > UINT32_MAX)
                throw E_UNEXPECTED;
            auto allocated = SvgFontAllocator<ByteOwner>(&memory, Font).allocate(1);
            owner = new (allocated) ByteOwner(memory, owner_receipt, font.bytes);
            ++statistics.byte_owners_created;
            auto bytes = owner->BytesView();
            Work(memory, 0);
            Api(loader->CreateInMemoryFontFileReference(factory, bytes.data(), static_cast<UINT32>(bytes.size()), owner,
                                                        &file));
            BOOL supported = FALSE;
            DWRITE_FONT_FILE_TYPE file_type{};
            DWRITE_FONT_FACE_TYPE face_type{};
            UINT32 count = 0;
            Work(memory, 0);
            Api(file->Analyze(&supported, &file_type, &face_type, &count));
            if (!supported || file_type != DWRITE_FONT_FILE_TYPE_CFF || face_type != DWRITE_FONT_FACE_TYPE_CFF ||
                count != 1)
                throw E_UNEXPECTED;
            Work(memory, 0);
            Api(factory->CreateFontFace(face_type, 1, &file, 0, DWRITE_FONT_SIMULATIONS_NONE, &face));
            DWRITE_FONT_METRICS metrics{};
            face->GetMetrics(&metrics);
            uint16_t maximum = 0;
            for (uint32_t i = 0; i < NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS; ++i)
                if (font.encoded_mask & (uint32_t{1} << i))
                {
                    auto const &glyph = font.glyphs[i];
                    if (!glyph.glyph || glyph.reserved || !(glyph.em_size > 0) || !std::isfinite(glyph.em_size) ||
                        !std::isfinite(glyph.baseline_x) || !std::isfinite(glyph.baseline_y))
                        throw E_UNEXPECTED;
                    maximum = std::max(maximum, glyph.glyph);
                }
            if (face->GetIndex() || face->GetSimulations() != DWRITE_FONT_SIMULATIONS_NONE ||
                face->GetGlyphCount() != uint32_t{maximum} + 1 || metrics.designUnitsPerEm != 2048)
                throw E_UNEXPECTED;
            Work(memory, 0);
            Api(static_cast<IDWriteFactory *>(factory)->CreateCustomRenderingParams(
                1, 0, 0, DWRITE_PIXEL_GEOMETRY_FLAT, DWRITE_RENDERING_MODE_OUTLINE, &rendering));
            Work(memory, 0);
            Api(CoCreateInstance(CLSID_WICImagingFactory2, nullptr, CLSCTX_INPROC_SERVER, __uuidof(IWICImagingFactory),
                                 reinterpret_cast<void **>(&wic)));
            EnsureD2D();
        }

        void ReturnCapacity(notepads_markdown_svg_work_token const &lease, uint64_t reserved, uint64_t admitted) noexcept
        {
            memory.live[Scratch] -= admitted;
            HRESULT hr = memory.services.release(memory.services.context, lease);
            if (hr != S_OK)
            {
                ++memory.failed_releases;
                memory.failed_bytes += reserved;
                if (memory.cleanup_error == S_OK)
                    memory.cleanup_error = hr;
                CleanupError(hr);
            }
        }

        void EndDrawing() noexcept
        {
            if (drawing && target)
                CleanupError(target->EndDraw());
            drawing = false;
        }

        void FreeSurface() noexcept
        {
            Drop(bitmap_lock);
            EndDrawing();
            Drop(brush);
            Drop(target);
            Drop(bitmap);
            if (pixels_reserved)
            {
                ReturnCapacity(pixel_lease, pixel_reserved, pixel_admitted);
                pixels_reserved = false;
                pixel_reserved = pixel_admitted = 0;
            }
        }

        void ValidateParameters(ID2D1RenderTarget *canvas, ID2D1SolidColorBrush *white,
                                D2D1_ANTIALIAS_MODE geometry_mode)
        {
            canvas->GetTextRenderingParams(&actual_rendering);
            if (!actual_rendering)
                throw E_UNEXPECTED;
            Api(actual_rendering->QueryInterface(__uuidof(IDWriteRenderingParams1),
                                                 reinterpret_cast<void **>(&actual_rendering1)));
            Api(actual_rendering->QueryInterface(__uuidof(IDWriteRenderingParams2),
                                                 reinterpret_cast<void **>(&actual_rendering2)));
            Api(actual_rendering->QueryInterface(__uuidof(IDWriteRenderingParams3),
                                                 reinterpret_cast<void **>(&actual_rendering3)));
            FLOAT dpi_x = 0, dpi_y = 0;
            canvas->GetDpi(&dpi_x, &dpi_y);
            auto color = white->GetColor();
            if (std::bit_cast<uint32_t>(actual_rendering->GetGamma()) != std::bit_cast<uint32_t>(1.f) ||
                std::bit_cast<uint32_t>(actual_rendering->GetEnhancedContrast()) != 0 ||
                std::bit_cast<uint32_t>(actual_rendering1->GetGrayscaleEnhancedContrast()) !=
                    std::bit_cast<uint32_t>(1.f) ||
                std::bit_cast<uint32_t>(actual_rendering->GetClearTypeLevel()) != 0 ||
                actual_rendering->GetPixelGeometry() != DWRITE_PIXEL_GEOMETRY_FLAT ||
                actual_rendering->GetRenderingMode() != DWRITE_RENDERING_MODE_OUTLINE ||
                actual_rendering3->GetRenderingMode1() != DWRITE_RENDERING_MODE1_OUTLINE ||
                actual_rendering2->GetGridFitMode() != DWRITE_GRID_FIT_MODE_DEFAULT ||
                canvas->GetTextAntialiasMode() != D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE ||
                canvas->GetAntialiasMode() != geometry_mode || dpi_x != 96 || dpi_y != 96 || color.r != 1 ||
                color.g != 1 || color.b != 1 || color.a != 1 || white->GetOpacity() != 1)
                throw E_UNEXPECTED;
            Drop(actual_rendering3);
            Drop(actual_rendering2);
            Drop(actual_rendering1);
            Drop(actual_rendering);
        }

        void Configure(ID2D1RenderTarget *canvas, D2D1_ANTIALIAS_MODE geometry_mode) noexcept
        {
            canvas->SetDpi(96, 96);
            canvas->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            canvas->SetAntialiasMode(geometry_mode);
            canvas->SetTextRenderingParams(rendering);
        }

        void CreateSurface(uint32_t width, uint32_t height, bool make_brush
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                           ,
                           SvgCoverageTestHooks const &hooks
#endif
        )
        {
            if (!width || !height || width > INT_MAX || height > INT_MAX || bitmap || target || pixels_reserved)
                throw E_UNEXPECTED;
            uint64_t row = Bytes(width, 4);
            if (row > UINT32_MAX || row > UINT64_MAX / height)
                throw Quota;
            pixel_reserved = row * height;
            Check(memory.services.reserve(memory.services.context, Scratch, pixel_reserved, &pixel_lease));
            pixels_reserved = true;
            Work(memory, 0);
            Api(wic->CreateBitmap(width, height, GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad, &bitmap));
            WICRect rectangle{0, 0, static_cast<INT>(width), static_cast<INT>(height)};
            Work(memory, 0);
            HRESULT locked = bitmap->Lock(&rectangle, WICBitmapLockWrite, &bitmap_lock);
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            if (locked == S_OK && hooks.failure_point == 2)
                locked = E_FAIL;
#endif
            Api(locked);
            UINT stride = 0;
            Api(bitmap_lock->GetStride(&stride));
            uint64_t capacity = Bytes(stride, height);
            if (stride < row || capacity > pixel_reserved)
                throw Quota;
            // GetStride describes only this short lock. It is admission
            // metadata, never a persistent backing/pitch or physical-RAM proof.
            Check(memory.services.admit(memory.services.context, pixel_lease, capacity));
            pixel_admitted = capacity;
            memory.live[Scratch] += capacity;
            Drop(bitmap_lock);
            auto properties = D2D1::RenderTargetProperties(
                D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96,
                D2D1_RENDER_TARGET_USAGE_NONE, D2D1_FEATURE_LEVEL_DEFAULT);
            Work(memory, 0);
            Api(d2d->CreateWicBitmapRenderTarget(bitmap, properties, &target));
            auto actual_size = target->GetPixelSize();
            auto actual_format = target->GetPixelFormat();
            if (actual_size.width != width || actual_size.height != height ||
                actual_format.format != DXGI_FORMAT_B8G8R8A8_UNORM ||
                actual_format.alphaMode != D2D1_ALPHA_MODE_PREMULTIPLIED)
                throw E_UNEXPECTED;
            Configure(target, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            if (make_brush)
            {
                Work(memory, 0);
                Api(target->CreateSolidColorBrush(D2D1::ColorF(1, 1, 1, 1), &brush));
                ValidateParameters(target, brush, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            }
        }

        void CheckSourceSize(SvgCoverageSampling const &plan
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                             ,
                             SvgCoverageTestHooks const &hooks
#endif
        )
        {
            if (!maximum_bitmap_side)
            {
                CreateSurface(1, 1, false
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                              ,
                              hooks
#endif
                );
                uint32_t maximum = target->GetMaximumBitmapSize();
                if (!maximum)
                    throw E_UNEXPECTED;
                FreeSurface();
                if (cleanup != S_OK)
                    throw CleanupStopped{};
                maximum_bitmap_side = maximum;
            }
            if (!plan.source_width || !plan.source_height || plan.source_width > maximum_bitmap_side ||
                plan.source_height > maximum_bitmap_side || plan.source_width > INT_MAX || plan.source_height > INT_MAX)
                throw Quota;
        }

        void DrawWhole(uint32_t index, ID2D1RenderTarget *canvas, ID2D1SolidColorBrush *white,
                       SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                       ,
                       SvgCoverageTestHooks const &hooks
#endif
        )
        {
            Configure(canvas, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            ValidateParameters(canvas, white, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            auto const &matrix = matrices[index];
            D2D1_MATRIX_3X2_F transform{matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]};
            canvas->SetTransform(transform);
            D2D1_MATRIX_3X2_F actual{};
            canvas->GetTransform(&actual);
            if (std::memcmp(&actual, &transform, sizeof(actual)))
                throw E_UNEXPECTED;
            auto const &glyph = font.glyphs[index];
            FLOAT advance = 0;
            DWRITE_GLYPH_OFFSET offset{};
            DWRITE_GLYPH_RUN run{face, glyph.em_size, 1, &glyph.glyph, &advance, &offset, FALSE, 0};
            Work(memory, Bytes(plans[index].source_width, plans[index].source_height));
            Work(memory, 0);
            canvas->BeginDraw();
            drawing = true;
            canvas->Clear(D2D1::ColorF(0, 0));
            canvas->DrawGlyphRun(D2D1::Point2F(glyph.baseline_x, glyph.baseline_y), &run, white,
                                 DWRITE_MEASURING_MODE_NATURAL);
            ++statistics.glyph_draws;
            HRESULT ended = canvas->EndDraw();
            drawing = false;
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            if (ended == S_OK && hooks.failure_point == 1)
                ended = E_FAIL;
#endif
            Api(ended);
        }

        void PrepareSurface(uint32_t index, SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                            ,
                            SvgCoverageTestHooks const &hooks
#endif
        )
        {
            auto const &plan = plans[index];
            CheckSourceSize(plan
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                            ,
                            hooks
#endif
            );
            CreateSurface(plan.source_width, plan.source_height, true
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                          ,
                          hooks
#endif
            );
            uint32_t actual_maximum = target->GetMaximumBitmapSize();
            if (!actual_maximum)
                throw E_UNEXPECTED;
            if (plan.source_width > actual_maximum || plan.source_height > actual_maximum)
                throw Quota;
            DrawWhole(index, target, brush, statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                      ,
                      hooks
#endif
            );
        }

        SvgCoverageSourceWindow CopyTile(SvgCoverageSampling const &plan, uint32_t x, uint32_t y, uint32_t width,
                                         uint32_t height, SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                                         ,
                                         SvgCoverageTestHooks const &hooks
#endif
        )
        {
            constexpr uint32_t tile_stride = SourceSide * 4;
            constexpr uint32_t tile_bytes = tile_stride * SourceSide;
            if (width > SourceSide || height > SourceSide || uint64_t{x} + width > plan.source_width ||
                uint64_t{y} + height > plan.source_height || x > INT_MAX || y > INT_MAX || width > INT_MAX ||
                height > INT_MAX)
                throw E_INVALIDARG;
            if (!width || !height)
                return {x, y, width, height, tile_stride, {}};
            if (bitmap_lock || drawing)
                throw E_UNEXPECTED;
            if (tile.empty())
            {
                Work(memory, tile_bytes);
                tile.resize(tile_bytes, 0xA5);
            }
            uint64_t needed = Add(Bytes(tile_stride, uint64_t{height} - 1), Bytes(width, 4));
            if (tile.size() != tile_bytes || needed > tile.size() || tile.size() > tile.capacity() ||
                tile.size() > UINT32_MAX)
                throw E_UNEXPECTED;
            WICRect rectangle{static_cast<INT>(x), static_cast<INT>(y), static_cast<INT>(width),
                              static_cast<INT>(height)};
            // CopyPixels owns no caller output: the fixed paid heap tile is its
            // local origin, while x/y remain global solely for the exact kernel.
            Work(memory, Bytes(Bytes(width, height), 4));
            Work(memory, 0);
            HRESULT copied = bitmap->CopyPixels(&rectangle, tile_stride, static_cast<UINT>(tile.size()), tile.data());
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            if (copied == S_OK && hooks.failure_point == 4)
                copied = E_FAIL;
#endif
            Api(copied);
            ++statistics.windows;
            return {x, y, width, height, tile_stride, tile};
        }

#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        void LockReference(SvgCoverageSampling const &plan, SvgCoverageStatistics &statistics,
                           SvgCoverageTestHooks const &hooks)
        {
            WICRect rectangle{0, 0, static_cast<INT>(plan.source_width), static_cast<INT>(plan.source_height)};
            Work(memory, 0);
            HRESULT locked = bitmap->Lock(&rectangle, WICBitmapLockRead, &bitmap_lock);
            if (locked == S_OK && hooks.failure_point == 2)
                locked = E_FAIL;
            Api(locked);
            ++statistics.windows;
        }

        SvgCoverageSourceWindow ReferenceWindow(SvgCoverageSampling const &plan)
        {
            // Existing local-machine diagnostic only. GetDataPointer is not
            // part of the normal MTA path; normal code uses CopyPixels above.
            UINT stride = 0, size = 0;
            BYTE *data = nullptr;
            Api(bitmap_lock->GetStride(&stride));
            Api(bitmap_lock->GetDataPointer(&size, &data));
            uint64_t needed = Add(Bytes(stride, uint64_t{plan.source_height} - 1), Bytes(plan.source_width, 4));
            if (stride < Bytes(plan.source_width, 4) || needed > size || size > pixel_admitted || !data)
                throw E_UNEXPECTED;
            return {0, 0, plan.source_width, plan.source_height, stride, {data, size}};
        }
#endif

        void RenderPath(uint32_t index, std::span<uint8_t> output, SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                        ,
                        SvgCoverageTestHooks const &hooks
#endif
        )
        {
            auto const &path = paths[index];
            auto const &plan = plans[index];
            uint32_t columns = (std::min(GroupSide, path.width) + BlockSide - 1) / BlockSide;
            uint32_t rows = (std::min(GroupSide, path.height) + BlockSide - 1) / BlockSide;
            size_t required = size_t{columns} * rows * 256;
            if (sums.size() < required)
            {
                if (sums.capacity() < required)
                    Work(memory, Bytes(sums.size(), sizeof(SvgCoverageWide)));
                Work(memory, Bytes(required - sums.size(), sizeof(SvgCoverageWide)));
                sums.resize(required);
            }
            uint32_t tile_side = SourceSide;
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            bool full = hooks.full_surface_reference != 0;
            tile_side = hooks.source_tile_side;
            if (tile_side != 256 && tile_side != 127)
                throw E_INVALIDARG;
#endif
            PrepareSurface(index, statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                           ,
                           hooks
#endif
            );
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            if (full)
            {
                LockReference(plan, statistics, hooks);
                if (hooks.window_ready)
                    hooks.window_ready(hooks.context, ReferenceWindow(plan));
            }
#endif
            for (uint64_t gy = 0; gy < path.height; gy += GroupSide)
            {
                for (uint64_t gx = 0; gx < path.width; gx += GroupSide)
                {
                    uint32_t group_width = static_cast<uint32_t>(std::min<uint64_t>(GroupSide, path.width - gx));
                    uint32_t group_height = static_cast<uint32_t>(std::min<uint64_t>(GroupSide, path.height - gy));
                    uint32_t group_columns = (group_width + BlockSide - 1) / BlockSide;
                    uint32_t group_rows = (group_height + BlockSide - 1) / BlockSide;
                    for (size_t i = 0; i < size_t{group_columns} * group_rows * 256; i += 256)
                    {
                        Work(memory, Bytes(256, sizeof(SvgCoverageWide)));
                        std::fill_n(sums.data() + i, 256, SvgCoverageWide{});
                    }
                    uint64_t x_begin = gx * plan.numerator / plan.denominator;
                    uint64_t y_begin = gy * plan.numerator / plan.denominator;
                    uint64_t x_end = ((gx + group_width) * plan.numerator + plan.denominator - 1) / plan.denominator;
                    uint64_t y_end = ((gy + group_height) * plan.numerator + plan.denominator - 1) / plan.denominator;
                    auto accumulate = [&](SvgCoverageSourceWindow const &window)
                    {
                        Work(memory, uint64_t{group_columns} * group_rows);
                        for (uint32_t by = 0; by < group_height; by += BlockSide)
                            for (uint32_t bx = 0; bx < group_width; bx += BlockSide)
                            {
                                SvgCoverageTargetBlock block{
                                    static_cast<uint32_t>(gx + bx), static_cast<uint32_t>(gy + by),
                                    std::min(BlockSide, group_width - bx), std::min(BlockSide, group_height - by)};
                                uint64_t left = uint64_t{block.x} * plan.numerator;
                                uint64_t right = (uint64_t{block.x} + block.width) * plan.numerator;
                                uint64_t top = uint64_t{block.y} * plan.numerator;
                                uint64_t bottom = (uint64_t{block.y} + block.height) * plan.numerator;
                                if (left >= (uint64_t{window.x} + window.width) * plan.denominator ||
                                    right <= uint64_t{window.x} * plan.denominator ||
                                    top >= (uint64_t{window.y} + window.height) * plan.denominator ||
                                    bottom <= uint64_t{window.y} * plan.denominator)
                                    continue;
                                size_t slot = ((by / BlockSide) * group_columns + bx / BlockSide) * 256;
                                AccumulateSvgCoverageWindow(plan, block, window,
                                                            {sums.data() + slot, size_t{block.width} * block.height},
                                                            PixelWork, &memory);
                            }
                    };
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                    if (full)
                        accumulate(ReferenceWindow(plan));
                    else
#endif
                    {
                        for (uint64_t y = y_begin; y < y_end; y += tile_side)
                            for (uint64_t x = x_begin; x < x_end; x += tile_side)
                            {
                                uint32_t width = static_cast<uint32_t>(std::min<uint64_t>(tile_side, x_end - x));
                                uint32_t height = static_cast<uint32_t>(std::min<uint64_t>(tile_side, y_end - y));
                                auto window = CopyTile(plan, static_cast<uint32_t>(x), static_cast<uint32_t>(y), width,
                                                       height, statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                                                       ,
                                                       hooks
#endif
                                );
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                                if (hooks.window_ready)
                                    hooks.window_ready(hooks.context, window);
#endif
                                accumulate(window);
                            }
                    }
                    for (uint32_t by = 0; by < group_height; by += BlockSide)
                        for (uint32_t bx = 0; bx < group_width; bx += BlockSide)
                        {
                            SvgCoverageTargetBlock block{static_cast<uint32_t>(gx + bx), static_cast<uint32_t>(gy + by),
                                                         std::min(BlockSide, group_width - bx),
                                                         std::min(BlockSide, group_height - by)};
                            size_t slot = ((by / BlockSide) * group_columns + bx / BlockSide) * 256;
                            ResolveSvgCoverageBlock(plan, block,
                                                    {sums.data() + slot, size_t{block.width} * block.height}, output,
                                                    path.stride, PixelWork, &memory);
                        }
                }
            }
            FreeSurface();
            if (cleanup != S_OK)
                throw CleanupStopped{};
        }
        void Execute(MsSvgCoverageBatch const &batch, MsSvgCoverageResult &pending,
                     SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                     ,
                     SvgCoverageTestHooks const &hooks, IUnknown *&retained
#endif
        )
        {
            // These bounded stack descriptors are synchronous borrows only.
            // No State member, sink, owner, or retirement receipt stores them.
            std::array<SvgCoverageFontInput, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> inputs{};
            size_t input_count = 0;
            for (uint32_t i = 0; i < batch.path_count; ++i)
            {
                Work(memory, sizeof(paths[i]));
                paths[i] = batch.paths[i];
                auto const &path = paths[i];
                std::span<MsSvgCoverageCommand const> commands;
                if (path.command_count)
                    commands = {batch.commands + path.command_start, path.command_count};
                bool moved = false, drawn = false;
                for (size_t j = 0; j < commands.size(); ++j)
                {
                    if (j % 128 == 0)
                        Work(memory, Bytes(std::min<size_t>(128, commands.size() - j), sizeof(commands[0])));
                    auto const &command = commands[j];
                    uint32_t used = CoordinateCount(command.verb);
                    if (command.reserved || (!moved && command.verb != NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE))
                        throw E_INVALIDARG;
                    for (uint32_t k = 0; k < 6; ++k)
                        if ((k < used && !std::isfinite(Value(command.xy_bits[k]))) ||
                            (k >= used && command.xy_bits[k]))
                            throw E_INVALIDARG;
                    moved |= command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE;
                    drawn |= command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE || command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD ||
                             command.verb == NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC;
                }
                if (drawn && path.width && path.height)
                    plans[i] = PlanSvgCoverageSampling(path.s_bits, path.t_bits, path.width, path.height);
                uint32_t bit = uint32_t{1} << i;
                if (!drawn || !path.width || !path.height)
                    pending.ready_mask |= bit;
                else if (!PrepareMatrix(i, commands))
                {
                    pending.ineligible_mask |= bit;
                    pending.reasons[i] = NOTEPADS_MARKDOWN_SVG_COVERAGE_PRECISION_DOMAIN;
                }
                else
                {
                    if (path.fill_rule)
                    {
                        if (!Normalize(i, commands))
                        {
                            pending.ineligible_mask |= bit;
                            pending.reasons[i] = NOTEPADS_MARKDOWN_SVG_COVERAGE_PRECISION_DOMAIN;
                            continue;
                        }
                        ++statistics.normalized_paths;
                        commands = sinks[i]->Commands();
                        if (!PrepareMatrix(i, commands))
                        {
                            pending.ineligible_mask |= bit;
                            pending.reasons[i] = NOTEPADS_MARKDOWN_SVG_COVERAGE_PRECISION_DOMAIN;
                            continue;
                        }
                    }
                    inputs[input_count++] = {&paths[i], commands, i};
                }
            }
            if (input_count)
            {
                BuildSvgCoverageFont({inputs.data(), input_count}, memory, font);
                pending.ready_mask |= font.encoded_mask | font.empty_mask;
                pending.ineligible_mask |= font.ineligible_mask;
                for (uint32_t i = 0; i < batch.path_count; ++i)
                    if (font.ineligible_mask & (uint32_t{1} << i))
                        pending.reasons[i] = font.reasons[i];
                if (font.encoded_mask)
                {
                    ++statistics.generated_fonts;
                    LoadFont(statistics);
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                    if (hooks.font_ready)
                        hooks.font_ready(hooks.context, owner->BytesView(), font.glyphs, face);
                    if (hooks.retain_byte_owner)
                    {
                        owner->AddRef();
                        retained = static_cast<IUnknown *>(owner);
                    }
#endif
                }
            }
            for (uint32_t i = 0; i < batch.path_count; ++i)
            {
                uint32_t bit = uint32_t{1} << i;
                auto const &path = paths[i];
                if (!(pending.ready_mask & bit) || !path.width || !path.height)
                    continue;
                std::span<uint8_t> output{batch.a8 + static_cast<size_t>(path.a8_offset),
                                          static_cast<size_t>(path.length)};
                if (font.encoded_mask & bit)
                    RenderPath(i, output, statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                               ,
                               hooks
#endif
                    );
                else
                {
                    for (uint32_t y = 0; y < path.height; ++y)
                        for (uint64_t x = 0; x < path.width; x += 4096)
                        {
                            auto count = static_cast<size_t>(std::min<uint64_t>(4096, path.width - x));
                            Work(memory, count);
                            std::memset(output.data() + static_cast<size_t>(uint64_t{y} * path.stride + x), 0, count);
                        }
                }
            }
            uint32_t expected = batch.path_count == 32 ? UINT32_MAX : (uint32_t{1} << batch.path_count) - 1;
            if ((pending.ready_mask & pending.ineligible_mask) ||
                (pending.ready_mask | pending.ineligible_mask) != expected)
                throw E_UNEXPECTED;
            Work(memory, 0);
        }

        void Clean(SvgCoverageStatistics &statistics
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                   ,
                   SvgCoverageTestHooks const &hooks
#endif
                   ) noexcept
        {
            // No cancellation checks during cleanup, and no borrowed caller data.
            FreeSurface();
            Drop(actual_rendering3);
            Drop(actual_rendering2);
            Drop(actual_rendering1);
            Drop(actual_rendering);
            Drop(geometry_sink);
            Drop(geometry);
            Drop(rendering);
            Drop(face);
            Drop(file);
            if (registered)
            {
                HRESULT hr = factory->UnregisterFontFileLoader(loader);
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                if (hr == S_OK && hooks.failure_point == 3)
                    hr = E_FAIL;
#endif
                registered = false;
                if (hr != S_OK)
                {
                    CleanupError(hr);
                    quarantine = true;
                }
            }
            Drop(loader);
            Drop(factory);
            Drop(wic);
            Drop(d2d);
            if (owner)
            {
                if (owner->References() == 1)
                {
                    Drop(owner);
                    if (owner_receipt.freed)
                        ++statistics.byte_owners_freed;
                    else
                    {
                        dependency = true;
                        CleanupError(E_UNEXPECTED);
                    }
                }
                else
                {
                    dependency = quarantine = true;
                    ++statistics.quarantined_owners;
                    CleanupError(E_UNEXPECTED);
                }
            }
            for (size_t i = 0; i < sinks.size(); ++i)
                if (sinks[i])
                {
                    if (sinks[i]->References() == 1)
                    {
                        Drop(sinks[i]);
                        if (!sink_receipts[i].freed)
                        {
                            dependency = true;
                            CleanupError(E_UNEXPECTED);
                        }
                    }
                    else
                    {
                        dependency = quarantine = true;
                        ++statistics.quarantined_owners;
                        CleanupError(E_UNEXPECTED);
                    }
                }
            // Non-COM owned allocations can always be physically freed now.
            Free(font.bytes);
            Free(sums);
            Free(tile);
            if (dependency)
                quarantine = true;
        }
    };

    SvgCoverageSession::SvgCoverageSession(SvgFontAllocationContext *memory) noexcept : memory_(memory)
    {
    }

    SvgCoverageSession::~SvgCoverageSession() noexcept
    {
        // The canonical Finish guard preserves the enclosing Impl whenever this
        // predicate is true. Quarantined State is deliberately never retried.
    }

    HRESULT SvgCoverageSession::CoverBatch(MsSvgCoverageBatch const *batch, MsSvgCoverageResult *result) noexcept
    {
        if (!memory_ || quarantined_)
            return Closed;
        if (working_ || state_)
            return RecordFault(memory_->services.context, E_UNEXPECTED);
        if (first_work_fault_ != S_OK)
            return first_work_fault_;
        if (cleanup_fault_ != S_OK)
            return cleanup_fault_;
        working_ = true;
        std::array<uint64_t, 7> before{};
        std::copy_n(memory_->live, before.size(), before.begin());
        uint64_t failed_before = memory_->failed_releases;
        MsSvgCoverageResult pending{};
        pending.size = sizeof(pending);
        pending.version = NOTEPADS_MARKDOWN_SVG_COVERAGE_ABI;
        HRESULT work = S_OK;
        try
        {
            Work(*memory_, 0);
            ValidateBatch(batch, result, *memory_);
            auto allocated = SvgFontAllocator<State>(memory_, Scratch).allocate(1);
            state_ = new (allocated) State(*memory_);
            ++statistics_.batches;
            state_->Execute(*batch, pending, statistics_
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                            ,
                            test_hooks_, retained_test_owner_
#endif
            );
        }
        catch (CleanupStopped const &)
        {
            // Execute stopped because a real physical-free/refund failed.
            // Its separate cleanup channel already contains the failure.
        }
        catch (HRESULT hr)
        {
            work = hr < 0 ? hr : E_UNEXPECTED;
        }
        catch (std::bad_alloc const &)
        {
            work = E_OUTOFMEMORY;
        }
        catch (...)
        {
            work = E_UNEXPECTED;
        }
        // Record work failure before any owner/free/refund can add a cleanup fault.
        if (work != S_OK)
        {
            if (first_work_fault_ == S_OK)
                first_work_fault_ = work;
            RecordFault(memory_->services.context, work);
        }
        if (state_)
        {
            state_->Clean(statistics_
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                          ,
                          test_hooks_
#endif
            );
            if (state_->cleanup != S_OK && cleanup_fault_ == S_OK)
                cleanup_fault_ = state_->cleanup;
            quarantined_ = state_->quarantine || state_->dependency;
            if (!quarantined_)
            {
                auto retired = std::exchange(state_, nullptr);
                retired->~State();
                SvgFontAllocator<State>(memory_, Scratch).deallocate(retired, 1);
            }
        }
        if (memory_->failed_releases != failed_before && cleanup_fault_ == S_OK)
            cleanup_fault_ = memory_->cleanup_error != S_OK ? memory_->cleanup_error : E_UNEXPECTED;
        if (!quarantined_ && !std::equal(before.begin(), before.end(), std::begin(memory_->live)))
        {
            if (cleanup_fault_ == S_OK)
                cleanup_fault_ = E_UNEXPECTED;
            RecordFault(memory_->services.context, cleanup_fault_);
        }
        working_ = false;
        if (work != S_OK)
            return work;
        if (cleanup_fault_ != S_OK)
            return cleanup_fault_;
        if (quarantined_)
            return E_UNEXPECTED;
        pending.completed = 1;
        *result = pending;
        return S_OK;
    }

    bool SvgCoverageSession::HasContextDependency() const noexcept
    {
        return state_ != nullptr;
    }
    bool SvgCoverageSession::IsQuarantined() const noexcept
    {
        return quarantined_;
    }
    bool SvgCoverageSession::IsWorking() const noexcept
    {
        return working_;
    }
    HRESULT SvgCoverageSession::FirstWorkFault() const noexcept
    {
        return first_work_fault_;
    }
    HRESULT SvgCoverageSession::CleanupFault() const noexcept
    {
        return cleanup_fault_;
    }
    SvgCoverageStatistics SvgCoverageSession::Statistics() const noexcept
    {
        return statistics_;
    }
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
    void SvgCoverageSession::SetTestHooks(SvgCoverageTestHooks const &hooks) noexcept
    {
        if (!working_ && !quarantined_)
            test_hooks_ = hooks;
    }
    IUnknown *SvgCoverageSession::TakeRetainedTestOwner() noexcept
    {
        return std::exchange(retained_test_owner_, nullptr);
    }
#endif
}

#pragma float_control(pop)
