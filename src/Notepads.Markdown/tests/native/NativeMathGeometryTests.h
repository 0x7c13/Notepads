// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"

namespace Notepads::Markdown::Native::MathGeometryTests
{
    // Fixed original numeric fixtures, real DWrite layout and WIC/D2D software
    // pixels and independent renderer baseline callbacks. Requires COM initialized.
    // No window, engine, font file or SVG.
    // Throws on failure; can run inside the existing Math snapshot case.
    void Run(std::shared_ptr<ReadingMemory> const &memory);
}
