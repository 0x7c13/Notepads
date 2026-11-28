// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Svg/NativeSvgFontMemory.h"
#include "notepads/markdown/svg_coverage.h"
#include <array>
#include <span>

namespace Notepads::Markdown::Native
{
    // EvenOdd normalization, when required, has already produced NonZero local
    // commands. The original M and density remain unchanged after normalization.
    struct SvgCoverageFontInput
    {
        MsSvgCoveragePath const* path;
        std::span<MsSvgCoverageCommand const> commands;
        uint32_t request_index;
    };
    struct SvgCoverageGlyph
    {
        float baseline_x{}, baseline_y{}, em_size{};
        uint16_t glyph{}, reserved{};
    };
    struct SvgCoverageFont
    {
        SvgFontVector<uint8_t> bytes;
        std::array<SvgCoverageGlyph, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> glyphs{};
        std::array<uint32_t, NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS> reasons{};
        uint32_t encoded_mask{}, empty_mask{}, ineligible_mask{};

        explicit SvgCoverageFont(SvgFontAllocationContext* context)
            : bytes(SvgFontAllocator<uint8_t>(context, SvgFontMemory::Font))
        {
        }
    };

    // Pure generated-font work; no installed/system font lookup or COM owner.
    // Actual allocation/admission/free/refund and bounded work use memory.
    // HRESULT exceptions are real failures; representation domains are explicit
    // result masks. Returned storage owns no borrowed input or callback state.
    void BuildSvgCoverageFont(std::span<SvgCoverageFontInput const> inputs,
                              SvgFontAllocationContext& memory, SvgCoverageFont& output);
}
