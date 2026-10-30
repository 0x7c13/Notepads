// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once
#include <unknwn.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Foundation.Metadata.h>
#include <winrt/Windows.Graphics.Display.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.Storage.Streams.h>
#include <winrt/Windows.UI.Text.Core.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Windows.UI.Xaml.Interop.h>
#include <winrt/Windows.UI.Popups.h>
#include <winrt/Windows.UI.ViewManagement.h>
#include <winrt/Windows.UI.Input.h>
#include <winrt/Windows.UI.Composition.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Automation.Peers.h>
#include <winrt/Windows.UI.Xaml.Automation.Provider.h>
#include <winrt/Windows.UI.Xaml.Automation.Text.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <winrt/Windows.UI.Xaml.Input.h>
#include <winrt/Windows.UI.Xaml.Controls.Primitives.h>
#include <winrt/Windows.UI.Xaml.Data.h>
#include <winrt/Windows.UI.Xaml.Markup.h>
#include <winrt/Windows.UI.Xaml.Navigation.h>
#include <winrt/Windows.UI.Xaml.Media.Imaging.h>

#include <windows.ui.xaml.media.dxinterop.h>

#define DUX Windows::UI::Xaml
#define DUXC Windows::UI::Xaml::Controls
#define DUI Windows::UI::Input
#define DCUR Windows::UI::Core::CoreCursorType
#define DUD Windows::System

#include <d3d11_4.h>
#include <d2d1_1.h>

#include <cstddef>
#include <cstdlib>
#include <cstdint>
#include <cassert>
#include <cstring>
#include <cstdio>
#include <cmath>
#include <climits>

#include <stdexcept>
#include <new>
#include <string>
#include <string_view>
#include <sstream>
#include <vector>
#include <map>
#include <set>
#include <queue>
#include <optional>
#include <algorithm>
#include <memory>
#include <chrono>
#include <mutex>
#include <functional>

#include <olectl.h>
#include <msctf.h>
#include <appmodel.h>

#include "ScintillaTypes.h"
#include "ScintillaMessages.h"
#include "ScintillaStructures.h"
#include "ILoader.h"
#include "ILexer.h"

#include "Debugging.h"
#include "Geometry.h"
#include "Platform.h"

#include "CharacterCategoryMap.h"
#include "Position.h"
#include "UniqueString.h"
#include "SplitVector.h"
#include "Partitioning.h"
#include "RunStyles.h"
#include "ContractionState.h"
#include "CellBuffer.h"
#include "CallTip.h"
#include "KeyMap.h"
#include "Indicator.h"
#include "LineMarker.h"
#include "Style.h"
#include "ViewStyle.h"
#include "CharClassify.h"
#include "Decoration.h"
#include "CaseFolder.h"
#include "Document.h"
#include "CaseConvert.h"
#include "UniConversion.h"
#include "Selection.h"
#include "PositionCache.h"
#include "EditModel.h"
#include "MarginView.h"
#include "EditView.h"
#include "Editor.h"
#include "ElapsedPeriod.h"

#include "AutoComplete.h"
#include "ScintillaBase.h"

#include "Scintilla.h"


#ifndef USE_DUMMY_PAGE
#include "ControlIncludes.h"
#endif
