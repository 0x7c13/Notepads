#include "pch.h"
#include "DummyPage.h"
#if __has_include("DummyPage.g.cpp")
#include "DummyPage.g.cpp"
#endif

// Workaround for https://github.com/microsoft/xlang/issues/431
// Improves compile time when working on headers by including
// them in the DummyPage header instead of pch.h
// because the XAML compiler is not including them itself

// Not used in Release mode
// Do not include in metadata
