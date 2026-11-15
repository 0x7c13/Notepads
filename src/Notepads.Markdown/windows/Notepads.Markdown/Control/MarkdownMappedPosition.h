// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownMappedPosition.g.h"
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownMappedPosition : MarkdownMappedPositionT<MarkdownMappedPosition>
    {
        MarkdownMappedPosition(guid document, uint64_t revision, uint64_t textGeneration, uint64_t sourceByteOffset, uint64_t previewUtf16Offset, Markdown::MarkdownPositionAffinity affinity, Markdown::MarkdownCorrespondence correspondence, uint32_t effects) noexcept
            : _document(document), _revision(revision), _textGeneration(textGeneration), _sourceByteOffset(sourceByteOffset), _previewUtf16Offset(previewUtf16Offset), _affinity(affinity), _correspondence(correspondence), _effects(effects)
        {
        }
        guid DocumentId() const noexcept
        {
            return _document;
        }
        uint64_t Revision() const noexcept
        {
            return _revision;
        }
        uint64_t TextGeneration() const noexcept
        {
            return _textGeneration;
        }
        uint64_t SourceByteOffset() const noexcept
        {
            return _sourceByteOffset;
        }
        uint64_t PreviewUtf16Offset() const noexcept
        {
            return _previewUtf16Offset;
        }
        Markdown::MarkdownPositionAffinity Affinity() const noexcept
        {
            return _affinity;
        }
        Markdown::MarkdownCorrespondence Correspondence() const noexcept
        {
            return _correspondence;
        }
        uint32_t Effects() const noexcept
        {
            return _effects;
        }

      private:
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _sourceByteOffset;
        uint64_t _previewUtf16Offset;
        Markdown::MarkdownPositionAffinity _affinity;
        Markdown::MarkdownCorrespondence _correspondence;
        uint32_t _effects;
    };
}
