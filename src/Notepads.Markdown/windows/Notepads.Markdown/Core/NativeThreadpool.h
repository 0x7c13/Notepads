// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <windows.h>

namespace Notepads::Markdown::Native
{
    inline bool PinModuleUntilCallbackReturns(PTP_CALLBACK_INSTANCE instance) noexcept
    {
        // The caller's original context owner/COM pin must remain live through
        // this acquisition. Failure must retain that owner and its real fees.
        auto library = LoadPackagedLibrary(L"Notepads.Markdown.dll", 0);
        if (!library)
            return false;
        // Explicitly transfer this one loader reference to the OS callback
        // epilogue. Disassociation and CloseThreadpool* do not shorten it.
        FreeLibraryWhenCallbackReturns(instance, library);
        return true;
    }
}