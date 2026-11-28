// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "CoreValidationCheck.g.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct CoreValidationCheck : CoreValidationCheckT<CoreValidationCheck>
    {
        CoreValidationCheck(hstring name, bool passed, int32_t status, hstring detail);
        hstring Name() const;
        bool Passed() const noexcept;
        int32_t Status() const noexcept;
        hstring Detail() const;

    private:
        hstring _name;
        bool _passed;
        int32_t _status;
        hstring _detail;
    };
}
