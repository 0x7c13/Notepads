// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreValidationReport.g.h"
#include <vector>

namespace winrt::Notepads::Markdown::implementation
{
    struct CoreValidationReport : CoreValidationReportT<CoreValidationReport>
    {
        CoreValidationReport(uint32_t abiVersion, std::vector<Markdown::CoreValidationCheck> checks,
                             size_t expectedChecks = 28);
        uint32_t CoreAbiVersion() const noexcept;
        bool Passed() const noexcept;
        com_array<Markdown::CoreValidationCheck> Checks() const;

    private:
        uint32_t _abiVersion;
        bool _passed;
        std::vector<Markdown::CoreValidationCheck> _checks;
    };
}
