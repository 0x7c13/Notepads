// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

namespace WinUIEditor
{
	inline bool IsApartmentDisconnected(HRESULT result) noexcept
	{
		return result == RPC_E_DISCONNECTED || result == RO_E_CLOSED || result == CO_E_OBJNOTCONNECTED;
	}

	int ConvertFromDipToPixelUnit(float val, float dpiAdjustmentRatio, bool rounded = true);
	bool IsClassicWindow();
	winrt::Windows::System::VirtualKeyModifiers GetKeyModifiersForCurrentThread();
}
