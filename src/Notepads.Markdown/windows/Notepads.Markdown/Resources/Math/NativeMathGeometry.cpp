// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathGeometry.h"
#include <array>
#include <new>

namespace Notepads::Markdown::Native
{
    MathAllocationOwner::operator bool() const noexcept
    {
        return static_cast<bool>(_source) || static_cast<bool>(_math);
    }

    bool MathAllocationOwner::IsMath() const noexcept
    {
        return static_cast<bool>(_math);
    }

    void *MathAllocationOwner::Allocate(size_t bytes, size_t alignment, MathMemoryFamily family, uint64_t slots) const
    {
        if (_math)
            return _math->Allocate(bytes, alignment, family, slots);
        if (!_source)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        if (alignment > alignof(std::max_align_t))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        return _source->Allocate(bytes);
    }

    void *MathAllocationOwner::AllocateInline(size_t bytes, size_t alignment) const
    {
        if (_math)
            return _math->AllocateInline(bytes, alignment);
        return Allocate(bytes, alignment, MathMemoryFamily::Metadata, 1);
    }

    void MathAllocationOwner::Free(void *value) const noexcept
    {
        if (_math)
            (void)NativeMathMemory::Free(value);
        else
            ReadingMemory::Free(value);
    }

    std::shared_ptr<ReadingMemory> const &MathAllocationOwner::Source() const noexcept
    {
        return _source;
    }

    namespace
    {
        bool FloatValue(double value) noexcept
        {
            return std::isfinite(value) && std::abs(value) <= std::numeric_limits<float>::max() &&
                   (value == 0 || static_cast<float>(value) != 0);
        }

        float Float(double value)
        {
            if (!FloatValue(value))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            return static_cast<float>(value);
        }

        bool Bounds(MathInkBounds const &value) noexcept
        {
            return FloatValue(value.left) && FloatValue(value.top) && FloatValue(value.right) &&
                   FloatValue(value.bottom) && value.left <= value.right && value.top <= value.bottom;
        }

        bool Color(MathRgba const &value) noexcept
        {
            auto channel = [](double number) { return std::isfinite(number) && number >= 0 && number <= 1; };
            return channel(value.red) && channel(value.green) && channel(value.blue) && channel(value.alpha);
        }

        bool Color(D2D1_COLOR_F const &value) noexcept
        {
            return Color(MathRgba{value.r, value.g, value.b, value.a});
        }

        bool Placement(MathPlacement const &value) noexcept
        {
            auto const &m = value.metrics;
            return value.status == MathPlacementStatus::Ready &&
                   (value.mode == MathPlacementMode::Inline || value.mode == MathPlacementMode::Display) &&
                   FloatValue(m.width) && m.width >= 0 && FloatValue(m.height) && m.height >= 0 &&
                   FloatValue(m.baseline) && m.baseline >= 0 && m.baseline <= m.height && !m.supportsSideways &&
                   FloatValue(value.emToDips) && value.emToDips > 0 && FloatValue(value.paintOriginX) &&
                   FloatValue(value.paintOriginY);
        }

        void Validate(MathArtifactView const &value)
        {
            if (value.primitives.size() > NativeMathDrawing::MaximumPrimitives ||
                value.commands.size() > NativeMathDrawing::MaximumCommands)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};
            if (!FloatValue(value.advanceEm) || value.advanceEm < 0 || !FloatValue(value.ascentEm) ||
                value.ascentEm < 0 || !FloatValue(value.descentEm) || value.descentEm < 0 || !Bounds(value.inkEm))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            for (auto const &command : value.commands)
            {
                if (!FloatValue(command.x1) || !FloatValue(command.y1) || !FloatValue(command.x2) ||
                    !FloatValue(command.y2) || !FloatValue(command.x3) || !FloatValue(command.y3))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                switch (command.verb)
                {
                case MathPathVerb::Move:
                case MathPathVerb::Line:
                case MathPathVerb::Quad:
                case MathPathVerb::Cubic:
                case MathPathVerb::Close:
                    break;
                default:
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_UNSUPPORTED};
                }
            }
            size_t referenced{};
            for (auto const &primitive : value.primitives)
            {
                // Subtraction checks avoid start+count wrap. Reused ranges also
                // consume the total replay-command allowance.
                if (primitive.start > value.commands.size() ||
                    primitive.count > value.commands.size() - primitive.start || primitive.count == 0)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
                if (primitive.count > NativeMathDrawing::MaximumCommands - referenced)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};
                referenced += primitive.count;
                if (!FloatValue(primitive.strokeWidthEm) || primitive.strokeWidthEm < 0 ||
                    (!primitive.fill && primitive.strokeWidthEm == 0) || !Color(primitive.color))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                if (primitive.paint != MathPaintRole::Foreground && primitive.paint != MathPaintRole::Rgba)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_UNSUPPORTED};
                bool open{}, segment{};
                for (auto const &command : value.commands.subspan(primitive.start, primitive.count))
                {
                    switch (command.verb)
                    {
                    case MathPathVerb::Move:
                        open = true;
                        break;
                    case MathPathVerb::Line:
                    case MathPathVerb::Quad:
                    case MathPathVerb::Cubic:
                        if (!open)
                            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                        segment = true;
                        break;
                    case MathPathVerb::Close:
                        if (!open)
                            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                        open = false;
                        break;
                    default:
                        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_UNSUPPORTED};
                    }
                }
                if (!segment)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            }
        }

        D2D1_POINT_2F Point(double x, double y)
        {
            return {Float(x), Float(y)};
        }

        void IncludeBounds(MathInkBounds &value, MathInkBounds const &bounds)
        {
            if (!Bounds(bounds))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            value.left = std::min(value.left, bounds.left);
            value.top = std::min(value.top, bounds.top);
            value.right = std::max(value.right, bounds.right);
            value.bottom = std::max(value.bottom, bounds.bottom);
        }

        MathInkBounds StrokeBounds(std::span<MathPathCommand const> commands, double width)
        {
            MathInkBounds result;
            bool first = true;
            auto point = [&](double x, double y)
            {
                // These are exactly the floats supplied to the geometry sink.
                double px = Float(x), py = Float(y);
                if (first)
                {
                    result = {px, py, px, py};
                    first = false;
                }
                else
                {
                    result.left = std::min(result.left, px);
                    result.top = std::min(result.top, py);
                    result.right = std::max(result.right, px);
                    result.bottom = std::max(result.bottom, py);
                }
            };
            for (auto const &command : commands)
            {
                if (command.verb == MathPathVerb::Close)
                    continue;
                point(command.x1, command.y1);
                if (command.verb == MathPathVerb::Quad || command.verb == MathPathVerb::Cubic)
                    point(command.x2, command.y2);
                if (command.verb == MathPathVerb::Cubic)
                    point(command.x3, command.y3);
            }
            // A Bezier lies in its control-point convex hull. Butt caps and
            // Bevel joins stay within radius width/2 of the stroked centreline.
            // No em-space flattening tolerance is used as a containment proof.
            double radius = static_cast<double>(Float(width)) / 2;
            result = {result.left - radius, result.top - radius, result.right + radius, result.bottom + radius};
            if (first || !Bounds(result))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            return result;
        }

        bool UpwardFloat(double value, float &result) noexcept
        {
            if (!FloatValue(value))
                return false;
            result = static_cast<float>(value);
            if (static_cast<double>(result) < value)
                result = std::nextafter(result, std::numeric_limits<float>::infinity());
            return std::isfinite(result);
        }

        class MathInline final : public IDWriteInlineObject
        {
            using FreeMemory = void (*)(void *) noexcept;
            std::atomic<ULONG> _references{1};
            FreeMemory _freeMemory;
            MathPlacement _placement;
            bool _terminal;

            static void FreeSource(void *memory) noexcept
            {
                ReadingMemory::Free(memory);
            }
            static void FreeMath(void *memory) noexcept
            {
                (void)NativeMathMemory::Free(memory);
            }

          public:
            MathInline(MathAllocationOwner const &allocation, MathPlacement const &placement, bool terminal) noexcept
                : _freeMemory(allocation.IsMath() ? FreeMath : FreeSource), _placement(placement), _terminal(terminal)
            {
            }
            static void *operator new(size_t bytes, MathAllocationOwner const &allocation)
            {
                return allocation.AllocateInline(bytes, alignof(MathInline));
            }
            static void operator delete(void *memory, MathAllocationOwner const &allocation) noexcept
            {
                allocation.Free(memory);
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteInlineObject))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteInlineObject *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return ++_references;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                auto count = --_references;
                if (!count)
                {
                    ModuleLifetime cleanupModule;
                    // The real source/math allocation prefix holds the lease.
                    // Keeping a source ReadingMemory here would unnecessarily
                    // extend its complete model lifetime beyond the old route.
                    auto freeMemory = _freeMemory;
                    this->~MathInline();
                    freeMemory(this);
                }
                return count;
            }
            HRESULT STDMETHODCALLTYPE Draw(void *, IDWriteTextRenderer *, FLOAT, FLOAT, BOOL, BOOL,
                                           IUnknown *) noexcept override
            {
                // The owner replays paths using TextAnchor after text drawing.
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = _placement.metrics;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                // The complete ink already belongs to the measured box.
                *output = {};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before,
                                                         DWRITE_BREAK_CONDITION *after) noexcept override
            {
                if (!before || !after)
                    return E_POINTER;
                *before = _placement.mode == MathPlacementMode::Display ? DWRITE_BREAK_CONDITION_MUST_BREAK
                                                                        : DWRITE_BREAK_CONDITION_NEUTRAL;
                *after = _placement.mode == MathPlacementMode::Display && !_terminal ? DWRITE_BREAK_CONDITION_MUST_BREAK
                                                                                     : DWRITE_BREAK_CONDITION_NEUTRAL;
                return S_OK;
            }
        };
    }

    std::shared_ptr<NativeMathDrawing> NativeMathDrawing::Create(std::shared_ptr<ReadingMemory> const &memory,
                                                                 ID2D1Factory *factory,
                                                                 MathArtifactView const &artifact)
    {
        if (!memory || !factory)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        Validate(artifact); // Reject the entire batch before allocating native geometry.
        return std::allocate_shared<NativeMathDrawing>(ReadingAllocator<NativeMathDrawing>{memory}, ConstructionKey{},
                                                       MathAllocationOwner{memory}, factory, artifact);
    }

    std::shared_ptr<NativeMathDrawing> NativeMathDrawing::Create(MathMemoryHandle const &memory, ID2D1Factory *factory,
                                                                 MathArtifactView const &artifact)
    {
        if (!memory || !factory)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        Validate(artifact);
        MathAllocationOwner allocation{memory};
        return std::allocate_shared<NativeMathDrawing>(
            MathGeometryAllocator<NativeMathDrawing>{allocation, MathMemoryFamily::Metadata}, ConstructionKey{},
            allocation, factory, artifact);
    }

    NativeMathDrawing::NativeMathDrawing(ConstructionKey, MathAllocationOwner const &allocation, ID2D1Factory *factory,
                                         MathArtifactView const &artifact)
        : _allocation(allocation), _advance(artifact.advanceEm), _ascent(artifact.ascentEm),
          _descent(artifact.descentEm), _ink(artifact.inkEm),
          _primitives(MathGeometryAllocator<MathPrimitive>{allocation, MathMemoryFamily::Mirror}),
          _commands(MathGeometryAllocator<MathPathCommand>{allocation, MathMemoryFamily::Path}),
          _paths(MathGeometryAllocator<winrt::com_ptr<ID2D1PathGeometry>>{allocation, MathMemoryFamily::Mirror})
    {
        _factory.copy_from(factory);
        _primitives.assign(artifact.primitives.begin(), artifact.primitives.end());
        _commands.assign(artifact.commands.begin(), artifact.commands.end());
        _paths.reserve(_primitives.size());
        _occupied = {std::min(0.0, _ink.left), std::min(-_ascent, _ink.top), std::max(_advance, _ink.right),
                     std::max(_descent, _ink.bottom)};
        auto style = D2D1::StrokeStyleProperties(D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_FLAT,
                                                 D2D1_LINE_JOIN_BEVEL, 1, D2D1_DASH_STYLE_SOLID, 0);
        winrt::check_hresult(factory->CreateStrokeStyle(style, nullptr, 0, _stroke.put()));
        for (auto const &primitive : _primitives)
        {
            winrt::com_ptr<ID2D1PathGeometry> path;
            winrt::check_hresult(factory->CreatePathGeometry(path.put()));
            winrt::com_ptr<ID2D1GeometrySink> sink;
            winrt::check_hresult(path->Open(sink.put()));
            sink->SetFillMode(D2D1_FILL_MODE_WINDING);
            bool open{};
            for (auto const &command : std::span{_commands}.subspan(primitive.start, primitive.count))
            {
                switch (command.verb)
                {
                case MathPathVerb::Move:
                    if (open)
                        sink->EndFigure(D2D1_FIGURE_END_OPEN);
                    sink->BeginFigure(Point(command.x1, command.y1),
                                      primitive.fill ? D2D1_FIGURE_BEGIN_FILLED : D2D1_FIGURE_BEGIN_HOLLOW);
                    open = true;
                    break;
                case MathPathVerb::Line:
                    sink->AddLine(Point(command.x1, command.y1));
                    break;
                case MathPathVerb::Quad:
                    sink->AddQuadraticBezier({Point(command.x1, command.y1), Point(command.x2, command.y2)});
                    break;
                case MathPathVerb::Cubic:
                    sink->AddBezier(
                        {Point(command.x1, command.y1), Point(command.x2, command.y2), Point(command.x3, command.y3)});
                    break;
                case MathPathVerb::Close:
                    sink->EndFigure(D2D1_FIGURE_END_CLOSED);
                    open = false;
                    break;
                default:
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_UNSUPPORTED};
                }
            }
            if (open)
                sink->EndFigure(D2D1_FIGURE_END_OPEN);
            winrt::check_hresult(sink->Close());
            D2D1_RECT_F bounds{};
            if (primitive.fill)
            {
                winrt::check_hresult(path->GetBounds(nullptr, &bounds));
                IncludeBounds(_occupied, {bounds.left, bounds.top, bounds.right, bounds.bottom});
            }
            if (primitive.strokeWidthEm > 0)
            {
                IncludeBounds(_occupied, StrokeBounds(std::span{_commands}.subspan(primitive.start, primitive.count),
                                                      primitive.strokeWidthEm));
            }
            _paths.push_back(std::move(path));
        }
    }

    MathArtifactView NativeMathDrawing::Artifact() const noexcept
    {
        return {_advance, _ascent, _descent, _ink, _primitives, _commands};
    }

    MathInkBounds NativeMathDrawing::OccupiedEm() const noexcept
    {
        return _occupied;
    }

    MathPlacement NativeMathDrawing::Place(double fontDips, double widthDips, MathPlacementMode mode,
                                           double displayBeforeDips, double displayAfterDips) const noexcept
    {
        MathPlacement result;
        result.mode = mode;
        if (!FloatValue(fontDips) || fontDips <= 0 || !FloatValue(widthDips) || widthDips <= 0 ||
            !FloatValue(displayBeforeDips) || displayBeforeDips < 0 || !FloatValue(displayAfterDips) ||
            displayAfterDips < 0 || (mode != MathPlacementMode::Inline && mode != MathPlacementMode::Display))
            return result;
        double naturalWidth = (_occupied.right - _occupied.left) * fontDips;
        double naturalHeight = (_occupied.bottom - _occupied.top) * fontDips;
        if (!FloatValue(naturalWidth) || !FloatValue(naturalHeight))
            return result;
        double fit = naturalWidth > widthDips ? widthDips / naturalWidth : 1;
        if (fit < (mode == MathPlacementMode::Display ? 0.75 : 1.0))
        {
            result.status = MathPlacementStatus::Limited;
            return result;
        }
        double desiredScale = fontDips * fit;
        double before = mode == MathPlacementMode::Display ? displayBeforeDips : 0;
        double after = mode == MathPlacementMode::Display ? displayAfterDips : 0;
        if (!FloatValue(desiredScale))
            return result;
        float scale = static_cast<float>(desiredScale);
        if (scale > desiredScale)
            scale = std::nextafter(scale, 0.0f);
        float available = static_cast<float>(widthDips);
        if (available > widthDips)
            available = std::nextafter(available, 0.0f);
        // Compute the box from the actual float scale/origin used by Replay.
        // Outward rounding preserves containment when a double becomes a float.
        // A bounded few-ULP correction leaves room for a rounded paint origin.
        for (unsigned attempt = 0; attempt != 8 && scale > 0 && available > 0; ++attempt)
        {
            if (mode == MathPlacementMode::Display && scale < fontDips * 0.75)
            {
                result.status = MathPlacementStatus::Limited;
                return result;
            }
            double width = (_occupied.right - _occupied.left) * scale;
            double gap = mode == MathPlacementMode::Display ? (available - width) / 2 : 0;
            float paintX{}, baseline{}, height{}, occupiedWidth{};
            if (gap >= 0 && UpwardFloat(-_occupied.left * scale + gap, paintX) &&
                UpwardFloat(-_occupied.top * scale + before, baseline) &&
                UpwardFloat(static_cast<double>(baseline) + _occupied.bottom * scale + after, height) &&
                UpwardFloat(static_cast<double>(paintX) + _occupied.right * scale, occupiedWidth) &&
                occupiedWidth <= available)
            {
                result.status = MathPlacementStatus::Ready;
                result.metrics = {mode == MathPlacementMode::Display ? available : occupiedWidth, height, baseline,
                                  FALSE};
                result.emToDips = scale;
                result.paintOriginX = paintX;
                return result;
            }
            scale = std::nextafter(scale, 0.0f);
        }
        return result;
    }

    winrt::com_ptr<IDWriteInlineObject> NativeMathDrawing::CreateInlineObject(
        std::shared_ptr<ReadingMemory> const &memory, MathPlacement const &placement, std::wstring_view layoutText,
        uint32_t objectOffset)
    {
        return CreateInlineObject(MathAllocationOwner{memory}, placement, layoutText, objectOffset);
    }

    winrt::com_ptr<IDWriteInlineObject> NativeMathDrawing::CreateInlineObject(MathMemoryHandle const &memory,
                                                                              MathPlacement const &placement,
                                                                              std::wstring_view layoutText,
                                                                              uint32_t objectOffset)
    {
        return CreateInlineObject(MathAllocationOwner{memory}, placement, layoutText, objectOffset);
    }

    winrt::com_ptr<IDWriteInlineObject> NativeMathDrawing::CreateInlineObject(MathAllocationOwner const &allocation,
                                                                              MathPlacement const &placement,
                                                                              std::wstring_view layoutText,
                                                                              uint32_t objectOffset)
    {
        if (!allocation || !Placement(placement))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        if (layoutText.size() > UINT32_MAX || objectOffset >= layoutText.size() ||
            layoutText[objectOffset] != L'\ufffc')
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        winrt::com_ptr<IDWriteInlineObject> result;
        result.attach(new (allocation) MathInline(allocation, placement, objectOffset + 1 == layoutText.size()));
        return result;
    }

    MathTextAnchor NativeMathDrawing::TextAnchor(IDWriteTextLayout *layout, uint32_t objectOffset) const
    {
        if (!layout)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        winrt::com_ptr<IDWriteInlineObject> object;
        DWRITE_TEXT_RANGE range{};
        winrt::check_hresult(layout->GetInlineObject(objectOffset, object.put(), &range));
        if (!object || range.startPosition != objectOffset || range.length != 1)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        UINT32 count{};
        auto status = layout->GetLineMetrics(nullptr, 0, &count);
        if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
            winrt::check_hresult(status);
        // Source-funded diagnostics preserve their original allocation route.
        // Finalized math payloads cannot grow a heap reservation during layout.
        constexpr size_t MaximumFundedLines = 256;
        std::array<DWRITE_LINE_METRICS, MaximumFundedLines> fixedLines{};
        ReadingVector<DWRITE_LINE_METRICS> sourceLines{ReadingAllocator<DWRITE_LINE_METRICS>{_allocation.Source()}};
        std::span<DWRITE_LINE_METRICS> lines;
        if (_allocation.IsMath())
        {
            if (count > fixedLines.size())
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};
            lines = std::span{fixedLines}.first(count);
        }
        else
        {
            sourceLines.resize(count);
            lines = sourceLines;
        }
        if (count)
            winrt::check_hresult(layout->GetLineMetrics(lines.data(), count, &count));
        DWRITE_TEXT_METRICS metrics{};
        winrt::check_hresult(layout->GetMetrics(&metrics));
        if (!FloatValue(metrics.top))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        uint64_t offset{};
        double top = metrics.top;
        for (auto const &line : lines)
        {
            if (!FloatValue(line.height) || line.height < 0 || !FloatValue(line.baseline) || line.baseline < 0 ||
                line.baseline > line.height || line.length > UINT32_MAX - offset)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
            if (objectOffset >= offset && objectOffset - offset < line.length)
            {
                FLOAT x{}, y{};
                DWRITE_HIT_TEST_METRICS hit{};
                winrt::check_hresult(layout->HitTestTextPosition(objectOffset, FALSE, &x, &y, &hit));
                return {Float(hit.left), Float(top + line.baseline)};
            }
            offset += line.length;
            top += line.height;
        }
        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
    }

    HRESULT NativeMathDrawing::Replay(ID2D1RenderTarget *target, MathPlacement const &placement, MathTextAnchor anchor,
                                      D2D1_COLOR_F foreground, bool forceForeground) const noexcept
    {
        if (!target || !Placement(placement) || !FloatValue(anchor.left) || !FloatValue(anchor.baseline) ||
            !Color(foreground))
            return E_INVALIDARG;
        winrt::com_ptr<ID2D1Factory> factory;
        target->GetFactory(factory.put());
        if (factory.get() != _factory.get())
            return D2DERR_WRONG_FACTORY;
        double x = static_cast<double>(anchor.left) + placement.paintOriginX;
        double y = static_cast<double>(anchor.baseline) + placement.paintOriginY;
        if (!FloatValue(x) || !FloatValue(y))
            return E_INVALIDARG;
        D2D1_MATRIX_3X2_F previous;
        target->GetTransform(&previous);
        auto transform = D2D1::Matrix3x2F::Scale(placement.emToDips, placement.emToDips) *
                         D2D1::Matrix3x2F::Translation(static_cast<float>(x), static_cast<float>(y)) * previous;
        if (!FloatValue(transform._11) || !FloatValue(transform._12) || !FloatValue(transform._21) ||
            !FloatValue(transform._22) || !FloatValue(transform._31) || !FloatValue(transform._32))
            return E_INVALIDARG;
        winrt::com_ptr<ID2D1SolidColorBrush> brush;
        auto status = target->CreateSolidColorBrush(foreground, brush.put());
        if (FAILED(status))
            return status;
        target->SetTransform(transform);
        for (size_t index = 0; index != _primitives.size(); ++index)
        {
            auto const &primitive = _primitives[index];
            if (!forceForeground && primitive.paint == MathPaintRole::Rgba)
            {
                auto const &c = primitive.color;
                brush->SetColor({static_cast<float>(c.red), static_cast<float>(c.green), static_cast<float>(c.blue),
                                 static_cast<float>(c.alpha)});
            }
            else
                brush->SetColor(foreground);
            if (primitive.fill)
                target->FillGeometry(_paths[index].get(), brush.get());
            if (primitive.strokeWidthEm > 0)
                target->DrawGeometry(_paths[index].get(), brush.get(), static_cast<float>(primitive.strokeWidthEm),
                                     _stroke.get());
        }
        target->SetTransform(previous);
        // The caller owns BeginDraw/EndDraw and observes deferred D2D errors.
        return S_OK;
    }
}
