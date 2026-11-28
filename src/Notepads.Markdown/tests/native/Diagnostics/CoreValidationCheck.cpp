// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Diagnostics/CoreValidationCheck.h"
#include "CoreValidationCheck.g.cpp"

namespace winrt::Notepads::Markdown::implementation
{
    CoreValidationCheck::CoreValidationCheck(hstring name, bool passed, int32_t status, hstring detail)
        : _name(std::move(name)), _passed(passed), _status(status), _detail(std::move(detail))
    {
    }

    hstring CoreValidationCheck::Name() const
    {
        return _name;
    }

    bool CoreValidationCheck::Passed() const noexcept
    {
        return _passed;
    }

    int32_t CoreValidationCheck::Status() const noexcept
    {
        return _status;
    }

    hstring CoreValidationCheck::Detail() const
    {
        return _detail;
    }
}
