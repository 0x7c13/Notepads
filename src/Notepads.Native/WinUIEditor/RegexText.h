// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <icu.h>
#include <cstdint>
#include <string_view>
namespace WinUIEditor
{
	struct SplitText
	{
		std::string_view first;
		std::string_view second;
		int64_t Length() const noexcept { return static_cast<int64_t>(first.size() + second.size()); }
		uint8_t At(int64_t index) const noexcept
		{
			return static_cast<uint8_t>(index < static_cast<int64_t>(first.size()) ? first[static_cast<size_t>(index)] : second[static_cast<size_t>(index) - first.size()]);
		}
	};
	UText *OpenSplitText(UText *text, SplitText source, UErrorCode *status) noexcept;
}
