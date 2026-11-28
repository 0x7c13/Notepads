// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Svg/NativeSvgFontMemory.h"
#include "notepads/markdown/svg_coverage.h"
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
#include "Resources/Svg/NativeSvgCoverageFont.h"
#include "Resources/Svg/NativeSvgCoverageSampling.h"
#include <dwrite.h>
#include <unknwn.h>
#endif

namespace Notepads::Markdown::Native
{
    struct SvgCoverageStatistics
    {
        uint64_t batches{}, glyph_draws{}, windows{}, normalized_paths{};
        uint64_t generated_fonts{}, byte_owners_created{}, byte_owners_freed{};
        uint64_t quarantined_owners{};
    };
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
    struct SvgCoverageTestHooks
    {
        // 0 normal; 1 force EndDraw work failure; 2 Lock work failure;
        // 3 Unregister cleanup failure; 4 CopyPixels work failure.
        // Injection does not replace actual cleanup. The fixed production
        // read tile is 256; tests also use the predeclared 127 partition.
        uint32_t failure_point{}, full_surface_reference{}, retain_byte_owner{};
        uint32_t source_tile_side{256};
        void *context{};
        void (*font_ready)(void *, std::span<uint8_t const>, std::span<SvgCoverageGlyph const>, IDWriteFontFace *){};
        void (*window_ready)(void *, SvgCoverageSourceWindow const &){};
    };
#endif

    // A member of the original canonical font session, not a font cache or queue.
    // Normal calls own all temporary state only until the synchronous return.
    class SvgCoverageSession final
    {
      public:
        explicit SvgCoverageSession(SvgFontAllocationContext *memory) noexcept;
        ~SvgCoverageSession() noexcept;
        SvgCoverageSession(SvgCoverageSession const &) = delete;
        SvgCoverageSession &operator=(SvgCoverageSession const &) = delete;

        HRESULT CoverBatch(MsSvgCoverageBatch const *, MsSvgCoverageResult *) noexcept;
        bool HasContextDependency() const noexcept;
        bool IsQuarantined() const noexcept;
        bool IsWorking() const noexcept;
        HRESULT FirstWorkFault() const noexcept;
        HRESULT CleanupFault() const noexcept;
        SvgCoverageStatistics Statistics() const noexcept;
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        // Test callbacks borrow data only while called; they may not reenter.
        void SetTestHooks(SvgCoverageTestHooks const &) noexcept;
        // Transfer the explicitly AddRef'd forced external reference to the test.
        IUnknown *TakeRetainedTestOwner() noexcept;
#endif

      private:
        struct State;
        SvgFontAllocationContext *memory_{};
        State *state_{};
        SvgCoverageStatistics statistics_{};
        HRESULT first_work_fault_{S_OK}, cleanup_fault_{S_OK};
        bool working_{}, quarantined_{};
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        SvgCoverageTestHooks test_hooks_{};
        IUnknown *retained_test_owner_{};
#endif
    };
}
