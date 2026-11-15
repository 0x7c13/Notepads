// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <winrt/base.h>

namespace Notepads::Markdown::Native
{
    // Native tasks and retained resources must keep their owning DLL loaded.
    struct ModuleLifetime
    {
        ModuleLifetime() noexcept
        {
            ++winrt::get_module_lock();
        }
        ~ModuleLifetime()
        {
            --winrt::get_module_lock();
        }
        ModuleLifetime(ModuleLifetime const &) = delete;
        ModuleLifetime &operator=(ModuleLifetime const &) = delete;
    };
}
