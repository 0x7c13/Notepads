// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Control/MarkdownView.h"
#include "Automation/ReadingObjectProvider.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct ReadingPeerCache
    {
        std::shared_ptr<TextSnapshot const> snapshot;
        ::Notepads::Markdown::Native::ReadingVector<com_ptr<implementation::ReadingObjectProvider>> peers;
        explicit ReadingPeerCache(std::shared_ptr<TextSnapshot const> owner)
            : snapshot(std::move(owner)),
              peers(::Notepads::Markdown::Native::ReadingAllocator<com_ptr<implementation::ReadingObjectProvider>>{
                  snapshot->main->Memory()})
        {
            auto objects = snapshot->descriptors->objects.size();
            auto links = snapshot->descriptors->links.size();
            auto disclosures = snapshot->descriptors->disclosures.size();
            auto tables = snapshot->descriptors->tables.size();
            if (links > SIZE_MAX - objects || disclosures > SIZE_MAX - objects - links ||
                tables > (SIZE_MAX - objects - links - disclosures) / 2)
                throw hresult_out_of_bounds();
            peers.resize(objects + links + disclosures + 2 * tables);
        }
    };

}
