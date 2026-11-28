// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <cstdint>
#include <memory>
#include <string>

namespace Notepads::Markdown::Native
{
    class MainTextSnapshot;
}

namespace Notepads::Markdown::Native::ReadingSearchTests
{
    struct Results
    {
        bool passed{};
        uint64_t comparisons{};
        std::wstring detail;
        bool permitsAndStops{};
        bool queryFundingFailure{};
        bool queryCapacityReleased{};
        uint64_t observedQueryCharge{};
    };

    // Uses the supplied actual Inline Main and its real ReadingMemory/model root.
    // Async operation and XAML publication lifetime checks belong to the host suite.
    Results Run(std::shared_ptr<MainTextSnapshot const> const &main);
}
