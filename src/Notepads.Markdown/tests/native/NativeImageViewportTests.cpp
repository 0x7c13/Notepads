// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageViewportTests.h"
#include "Resources/Images/NativeImageViewport.h"
#include <limits>
namespace Notepads::Markdown::Native::ImageViewportTests
{
    CoreValidation::CheckResult Run()
    {
        auto padded = ImageViewportWindow::FromCanvas(100, 100, 16);
        auto unpadded = ImageViewportWindow::FromCanvas(100, 100, 0);
        auto initial = ImageViewportWindow::FromCanvas(0, 100, 16);
        bool passed = padded && unpadded && initial && padded->top == 84 && padded->bottom == 184 &&
            padded->Visible(85, 95) && !padded->Visible(190, 200) && padded->Near(190, 200) &&
            padded->Distance(85, 95) == 0 && padded->Distance(190, 200) == 6 &&
            !unpadded->Visible(85, 95) && unpadded->Visible(190, 200) &&
            initial->top == -16 && initial->bottom == 84 && initial->Visible(0, 20) &&
            !initial->Visible(85, 95) && !padded->Near(-40, -20) && !padded->Near(290, 310) &&
            !ImageViewportWindow::FromCanvas(0, 0, 16) &&
            !ImageViewportWindow::FromCanvas(0, 100, std::numeric_limits<double>::quiet_NaN());
        return {L"image_viewport_canvas_padding_priority", passed, passed ? S_OK : E_FAIL,
            L"Actual visible/near admission helper: Canvas y100/H100/pad16 prioritizes document85..95 over190..200; zero-padding negative control and invalid geometry."};
    }
}
