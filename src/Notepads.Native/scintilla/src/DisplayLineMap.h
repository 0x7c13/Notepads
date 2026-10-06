// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <memory>
#include <vector>

namespace Scintilla::Internal
{
	struct DisplayGap { Sci::Line beforeLine{}, rows{}; };
	struct TintedLineRange { Sci::Line start{}, count{}; };
	// Intended for immutable unwrapped views. Fold/wrap mutations retire padding.
	std::unique_ptr<IContractionState> DisplayLineMapCreate(std::unique_ptr<IContractionState> base,
		std::vector<DisplayGap> gaps, std::vector<TintedLineRange> tints);
}
