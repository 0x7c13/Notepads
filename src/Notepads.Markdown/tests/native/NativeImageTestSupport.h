// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Resources/Images/NativeImageResources.h"

namespace Notepads::Markdown::Native
{
    // Fixtures use an explicit current target before requesting physical work.
    // The default preserves intrinsic raster pixels at the production side cap.
    inline std::shared_ptr<NativeImageRequest> StartImageTestRequest(
        std::shared_ptr<NativeImagePolicy> const &policy, uint32_t object,
        notepads_markdown_image_target const &target = {1, 1, ImageMaximumDecodeSide, 1},
        uint32_t kind = NOTEPADS_MARKDOWN_IMAGE_AUTO)
    {
        policy->Desire(object, target);
        return policy->Start(object, kind);
    }
}
