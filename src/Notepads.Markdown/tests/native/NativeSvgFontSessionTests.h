// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Svg/NativeSvgFontSession.h"

namespace Notepads::Markdown::Native
{
    inline constexpr uint32_t NativeSvgFontTestCaseCount = 29;
    struct NativeSvgFontTestIdentity
    {
        uint8_t sha256[32]{};
        uint64_t bytes{};
        uint32_t faceIndex{}, simulations{}, axisCount{};
        MsFrAxis axes[16]{};
    };
    struct NativeSvgFontTestWitness
    {
        wchar_t expectedFamily[64]{};
        uint32_t utf8Start{}, utf8Length{};
        NativeSvgFontTestIdentity expected{}, actual{};
        bool matched{};
    };
    struct NativeSvgFontTestResult
    {
        bool passed{}, planPartition{}, actualFontSha{}, fragmentPairing{}, syntheticExercised{};
        bool cleanupSucceeded{}, originalFaultPreserved{}, lateIsolated{}, wrongThreadRejected{};
        HRESULT createStatus{E_UNEXPECTED}, workStatus{E_UNEXPECTED}, cleanupStatus{E_UNEXPECTED};
        uint64_t session{}, files{}, faces{}, runs{}, fragmentReads{}, fragmentReleases{};
        uint64_t geometryCommands{}, failedReleases{};
        // Test-owned fixed receipts survive Plan/session retirement. Expectations
        // are captured from independently named DWrite faces before actual Plan.
        HRESULT prerequisiteStatus{E_UNEXPECTED};
        bool prerequisitesReady{}, selectedIdentitiesMatch{}, systemFallbackDistinct{}, failureOutputUntouched{};
        uint32_t prerequisiteStage{}, witnessCount{};
        NativeSvgFontTestWitness witnesses[3]{};
        NativeSvgFontTestIdentity systemFallback{};
        uint64_t referenceFragmentReads{}, referenceFragmentReleases{};
    };
    // Each case needs fresh real resources from a real model/request. Cancellation
    // uses the actual original Core token. No fake resource symbols are supplied.
    // 0 Latin, 1 mixed fallback, 2 requested synthetic font/metrics/geometry,
    // 3 invalid native draft, 4 real cancellation, 5 wrong-worker rejection,
    // 6 retired callback cannot affect a following actual Native session.
    // 7 real original token cancellation before Native Create.
    // 8..24 ordered/absent/partial lists, five generics, quoted generic,
    // canonical special names, mixed clusters/emoji, and RTL. Missing independent
    // font/coverage prerequisites fail explicitly; expectations never use Plan.
    // 25..28 reject bad escape, trailing empty item, unclosed string, surrogate
    // escape; original fault/output and real resource retirement are checked.
    NativeSvgFontTestResult RunNativeSvgFontSessionTest(notepads_markdown_svg_resources *resources, notepads_markdown_core_cancel const *cancel,
                                                        uint32_t testCase) noexcept;
}
