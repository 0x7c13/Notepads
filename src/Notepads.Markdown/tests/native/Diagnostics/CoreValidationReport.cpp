// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Diagnostics/CoreValidationReport.h"
#include "CoreValidationReport.g.cpp"
#include "notepads/markdown/core.h"

namespace winrt::Notepads::Markdown::implementation
{
    CoreValidationReport::CoreValidationReport(uint32_t abiVersion, std::vector<Markdown::CoreValidationCheck> checks,
                                             size_t expectedChecks)
        : _abiVersion(abiVersion), _passed(abiVersion == NOTEPADS_MARKDOWN_CORE_ABI_VERSION && checks.size() == expectedChecks),
          _checks(std::move(checks))
    {
        for (auto const &check : _checks)
        {
            _passed = _passed && check.Passed();
        }
    }

    uint32_t CoreValidationReport::CoreAbiVersion() const noexcept
    {
        return _abiVersion;
    }

    bool CoreValidationReport::Passed() const noexcept
    {
        return _passed;
    }

    com_array<Markdown::CoreValidationCheck> CoreValidationReport::Checks() const
    {
        return com_array<Markdown::CoreValidationCheck>(_checks.begin(), _checks.end());
    }
}
