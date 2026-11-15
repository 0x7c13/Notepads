// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include <string_view>

namespace Notepads::Markdown::Native::MeasurementCacheTests
{
    inline constexpr std::string_view FixtureSource =
        "# Heading `mono`\n\n[repeat 😀 é שלום](first)\n\n[repeat 😀 é שלום](second)\n\n"
        "| Center | Trailing |\n| :---: | ---: |\n| wrap wrap wrap wrap | value |\n| | |\n"
        "| left one two three four five six seven eight nine ten eleven twelve | "
        "right one two three four five six seven eight nine ten |\n";
    inline constexpr std::wstring_view FixtureMain =
        L"Heading mono\nrepeat 😀 é שלום\nrepeat 😀 é שלום\nCenter\tTrailing\nwrap wrap wrap wrap\tvalue\n\t\n"
        L"left one two three four five six seven eight nine ten eleven twelve\t"
        L"right one two three four five six seven eight nine ten";
    struct Results
    {
        bool geometryOracle{};
        bool currentLinks{};
        bool freshFrames{};
        bool configurationMiss{};
        bool tokenGuards{};
        bool epochAndContention{};
        bool budgets{};
        uint64_t hits{}, misses{}, nativeBytes{}, cacheBytes{};
        uint64_t lines{}, clusters{};
        uint64_t lineSize{}, lineCapacity{}, lineAllocationBytes{};
    };
    // Background-only; the supplied model must contain FixtureSource.
    Results Run(std::shared_ptr<ReadingMemory> const &memory);
}
