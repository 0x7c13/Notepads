#pragma once

#include "DummyPage.g.h"

#include "ControlIncludes.h"

namespace winrt::WinUIEditor::implementation
{
    struct DummyPage : DummyPageT<DummyPage>
    {
    };
}

namespace winrt::WinUIEditor::factory_implementation
{
    struct DummyPage : DummyPageT<DummyPage, implementation::DummyPage>
    {
    };
}
