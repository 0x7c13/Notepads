// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string>
#include <string_view>
#include <winrt/Notepads.Markdown.h>
#include "Diagnostics/CoreValidationSuite.h"

namespace Notepads::Markdown::Native::ReadingOutlineTests
{
    // Exact input/order/anchors from the pinned Comrak literal-suffix regression.
    // Main is an independent literal oracle, not the outline's generated text.
    inline constexpr std::string_view FixtureSource{"# a\n# a\n# a-1\n# a\n"};
    inline constexpr std::wstring_view FixtureMain{L"a\na\na-1\na"};
    // Existing Core block-separator rules only emit separators for blocks with
    // actual output. The two empty headings therefore share [0,0), and Tail
    // starts at 0 with no synthetic leading LF.
    inline constexpr std::string_view FixtureEmptySource{"#\n\n#\n\n# Tail\n"};
    inline constexpr std::wstring_view FixtureEmptyMain{L"Tail"};

    struct Results
    {
        bool passed{};
        uint32_t directoryChecks{};
        uint32_t fragmentChecks{};
        bool fundingFailure{};
        bool capacityReleased{};
        bool cancelledSourceKeepsReadOnly{};
        uint32_t geometryChecks{};
        bool emptyBlocksDistinct{};
        bool currentLayoutGeometry{};
        bool sameOutlineAcrossLayouts{};
        bool missingBlockRejected{};
        bool retainedFrameUnchanged{};
        std::wstring detail;
    };

    // Called off UI with this fixture's actual immutable Main and live source
    // flag. Current-view identity and WinRT pages are independent host/UI
    // gates; the real empty-block geometry case is a separate fixture below.
    Results Run(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag);
    // Uses two real Rust layouts / DWrite-measured Frames on the same source
    // state. Raw Frame block.y is the independent oracle for node navigation.
    Results RunEmptyGeometry(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag);
    // UI entry. Calls the actual IVectorView ABI with a valid external Heading
    // whose IUnknown QI releases the caller's last external page reference.
    bool RunPageReentrancy(winrt::Notepads::Markdown::MarkdownOutline const &snapshot,
                           std::shared_ptr<ReadingMemory> const &memory);
    // Reuses the reading suite's mounted isolated control. A real deferred
    // disclosure reveal is held until its expanded extent is ready, making
    // newer follow admission and stale rejection independent of XAML timing.
    winrt::Windows::Foundation::IAsyncAction RunFollowDisclosureNavigationAsync(
        winrt::Notepads::Markdown::MarkdownView view,
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<CoreValidation::Context> context);
}
