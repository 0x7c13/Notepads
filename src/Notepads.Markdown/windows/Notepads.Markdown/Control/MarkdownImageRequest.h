// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownImageRequest.g.h"
#include <utility>

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownImageRequest : MarkdownImageRequestT<MarkdownImageRequest>
    {
        MarkdownImageRequest(guid document, uint64_t revision, uint64_t textGeneration, uint64_t imageId, uint64_t requestId, uint64_t providerGeneration, hstring destination, hstring title, hstring alt, uint64_t maximumEncodedBytes, uint64_t maximumPixelCount) noexcept
            : _document(document), _revision(revision), _textGeneration(textGeneration), _imageId(imageId), _requestId(requestId), _providerGeneration(providerGeneration), _destination(std::move(destination)), _title(std::move(title)), _alt(std::move(alt)), _maximumEncodedBytes(maximumEncodedBytes), _maximumPixelCount(maximumPixelCount)
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
        uint64_t ImageId() const noexcept
        {
            return _imageId;
        }
        uint64_t RequestId() const noexcept
        {
            return _requestId;
        }
        uint64_t ProviderGeneration() const noexcept
        {
            return _providerGeneration;
        }
        hstring Destination() const
        {
            return _destination;
        }
        hstring Title() const
        {
            return _title;
        }
        hstring Alt() const
        {
            return _alt;
        }
        uint64_t MaximumEncodedBytes() const noexcept
        {
            return _maximumEncodedBytes;
        }
        uint64_t MaximumPixelCount() const noexcept
        {
            return _maximumPixelCount;
        }

      private:
        guid _document;
        uint64_t _revision;
        uint64_t _textGeneration;
        uint64_t _imageId;
        uint64_t _requestId;
        uint64_t _providerGeneration;
        hstring _destination;
        hstring _title;
        hstring _alt;
        uint64_t _maximumEncodedBytes;
        uint64_t _maximumPixelCount;
    };
}
