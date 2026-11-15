// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include "Resources/Math/NativeMathMemory.h"
#include <d2d1.h>
#include <dwrite.h>
#include <span>
#include <string_view>

namespace Notepads::Markdown::Native
{
    // Private geometry allocation authority. Exactly one real funding root is
    // held: the original source reservation or a single post-join math owner.
    // Ordinary reading allocations retain their existing ReadingMemory route.
    class MathAllocationOwner final
    {
      public:
        MathAllocationOwner() noexcept = default;
        explicit MathAllocationOwner(std::shared_ptr<ReadingMemory> source) noexcept : _source(std::move(source))
        {
        }
        explicit MathAllocationOwner(MathMemoryHandle math) noexcept : _math(std::move(math))
        {
        }
        explicit operator bool() const noexcept;
        bool IsMath() const noexcept;
        void *Allocate(size_t bytes, size_t alignment, MathMemoryFamily family, uint64_t slots) const;
        void *AllocateInline(size_t bytes, size_t alignment) const;
        void Free(void *value) const noexcept;
        std::shared_ptr<ReadingMemory> const &Source() const noexcept;
        bool operator==(MathAllocationOwner const &) const noexcept = default;

      private:
        std::shared_ptr<ReadingMemory> _source;
        MathMemoryHandle _math;
    };

    template <typename T> struct MathGeometryAllocator
    {
        using value_type = T;
        MathAllocationOwner owner;
        MathMemoryFamily family{MathMemoryFamily::Mirror};
        MathGeometryAllocator() noexcept = default;
        MathGeometryAllocator(MathGeometryAllocator const &) noexcept = default;
        MathGeometryAllocator &operator=(MathGeometryAllocator const &) noexcept = default;
        MathGeometryAllocator(MathGeometryAllocator &&other) noexcept : owner(other.owner), family(other.family)
        {
        }
        MathGeometryAllocator &operator=(MathGeometryAllocator &&other) noexcept
        {
            owner = other.owner;
            family = other.family;
            return *this;
        }
        explicit MathGeometryAllocator(MathAllocationOwner value,
                                       MathMemoryFamily kind = MathMemoryFamily::Mirror) noexcept
            : owner(std::move(value)), family(kind)
        {
        }
        template <typename U>
        MathGeometryAllocator(MathGeometryAllocator<U> const &other) noexcept : owner(other.owner), family(other.family)
        {
        }
        T *allocate(size_t count)
        {
            if (count == 0)
                return nullptr;
            if (count > std::numeric_limits<size_t>::max() / sizeof(T))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            return static_cast<T *>(owner.Allocate(count * sizeof(T), alignof(T), family, count));
        }
        void deallocate(T *value, size_t) noexcept
        {
            owner.Free(value);
        }
        template <typename U> bool operator==(MathGeometryAllocator<U> const &other) const noexcept
        {
            return owner == other.owner && family == other.family;
        }
    };
    template <typename T> using MathGeometryVector = std::vector<T, MathGeometryAllocator<T>>;

    // Private, platform-owned DTOs. Coordinates are em, baseline-relative, y down.
    // They contain no font/engine objects and are not the derived/public ABI.
    enum class MathPathVerb : uint32_t
    {
        Move,
        Line,
        Quad,
        Cubic,
        Close
    };

    struct MathPathCommand
    {
        MathPathVerb verb{};
        double x1{}, y1{}, x2{}, y2{}, x3{}, y3{};
    };

    struct MathInkBounds
    {
        double left{}, top{}, right{}, bottom{};
    };

    struct MathRgba
    {
        double red{}, green{}, blue{}, alpha{1};
    };

    enum class MathPaintRole : uint32_t
    {
        Foreground,
        Rgba
    };

    struct MathPrimitive
    {
        uint32_t start{}, count{};
        bool fill{true};
        double strokeWidthEm{};
        MathPaintRole paint{MathPaintRole::Foreground};
        MathRgba color{};
    };

    struct MathArtifactView
    {
        double advanceEm{}, ascentEm{}, descentEm{};
        MathInkBounds inkEm{};
        std::span<MathPrimitive const> primitives;
        std::span<MathPathCommand const> commands;
    };

    enum class MathPlacementMode : uint32_t
    {
        Inline,
        Display
    };

    enum class MathPlacementStatus : uint32_t
    {
        Invalid,
        Ready,
        Limited
    };

    struct MathPlacement
    {
        MathPlacementStatus status{MathPlacementStatus::Invalid};
        MathPlacementMode mode{MathPlacementMode::Inline};
        DWRITE_INLINE_OBJECT_METRICS metrics{};
        // One em->DIP scale; no later font or y-axis conversion is permitted.
        float emToDips{};
        // Relative to the inline box left and the actual DWrite line baseline.
        float paintOriginX{}, paintOriginY{};
    };

    struct MathTextAnchor
    {
        float left{}, baseline{};
    };

    class NativeMathDrawing final
    {
        struct ConstructionKey
        {
          private:
            friend class NativeMathDrawing;
            ConstructionKey() noexcept = default;
        };

      public:
        static constexpr size_t MaximumPrimitives = 2048;
        static constexpr size_t MaximumCommands = 65536;

        // Throws ReadingFailure for invalid/quota input, hresult_error for D2D.
        // The caller supplies the same factory as the eventual render target.
        // All copied arrays/owner headers are funded by ReadingMemory. D2D's
        // internal allocation/driver physical cost is not measured by this API.
        static std::shared_ptr<NativeMathDrawing> Create(std::shared_ptr<ReadingMemory> const &memory,
                                                         ID2D1Factory *factory, MathArtifactView const &artifact);
        // Same geometry and validation, with every known native capacity charged
        // to this one real math job's cumulative Path/Mirror/Metadata ledger.
        static std::shared_ptr<NativeMathDrawing> Create(MathMemoryHandle const &memory, ID2D1Factory *factory,
                                                         MathArtifactView const &artifact);
        NativeMathDrawing(ConstructionKey, MathAllocationOwner const &allocation, ID2D1Factory *factory,
                          MathArtifactView const &artifact);
        NativeMathDrawing(NativeMathDrawing const &) = delete;
        NativeMathDrawing &operator=(NativeMathDrawing const &) = delete;

        MathArtifactView Artifact() const noexcept;
        // Union of logical/declared ink, fill bounds and conservative stroke
        // control-point hulls. A stroke bound is containment, not tight ink.
        MathInkBounds OccupiedEm() const noexcept;
        // A display occupies width DIPs and may shrink to 75%, then Limited.
        // Optional display spacing contributes to natural line metrics only.
        MathPlacement Place(double fontDips, double widthDips, MathPlacementMode mode, double displayBeforeDips = 0,
                            double displayAfterDips = 0) const noexcept;

        // Does not alter text or insert line breaks. Install on exactly one
        // U+FFFC; layoutText must be the actual text passed to CreateTextLayout.
        // Only a display at its actual end has after=NEUTRAL, avoiding DWrite's
        // extra empty terminal line. Use content-based/default line spacing.
        static winrt::com_ptr<IDWriteInlineObject> CreateInlineObject(std::shared_ptr<ReadingMemory> const &memory,
                                                                      MathPlacement const &placement,
                                                                      std::wstring_view layoutText,
                                                                      uint32_t objectOffset);
        // Late math wrappers come only from the already prepaid fixed arena.
        static winrt::com_ptr<IDWriteInlineObject> CreateInlineObject(MathMemoryHandle const &memory,
                                                                      MathPlacement const &placement,
                                                                      std::wstring_view layoutText,
                                                                      uint32_t objectOffset);
        // Uses measured line baselines, not hit-test rectangle height/leading.
        // The finalized math route uses a fixed stack buffer for <=256 lines;
        // a larger layout is explicitly ResourceLimit, without a guessed anchor.
        MathTextAnchor TextAnchor(IDWriteTextLayout *layout, uint32_t objectOffset) const;
        HRESULT Replay(ID2D1RenderTarget *target, MathPlacement const &placement, MathTextAnchor anchor,
                       D2D1_COLOR_F foreground, bool forceForeground) const noexcept;

      private:
        static winrt::com_ptr<IDWriteInlineObject> CreateInlineObject(MathAllocationOwner const &allocation,
                                                                      MathPlacement const &placement,
                                                                      std::wstring_view layoutText,
                                                                      uint32_t objectOffset);
        MathAllocationOwner _allocation;
        winrt::com_ptr<ID2D1Factory> _factory;
        winrt::com_ptr<ID2D1StrokeStyle> _stroke;
        double _advance{}, _ascent{}, _descent{};
        MathInkBounds _ink{}, _occupied{};
        MathGeometryVector<MathPrimitive> _primitives;
        MathGeometryVector<MathPathCommand> _commands;
        MathGeometryVector<winrt::com_ptr<ID2D1PathGeometry>> _paths;
    };
}
