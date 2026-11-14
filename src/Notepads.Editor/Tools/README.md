# Editor interface generator

This Windows tool generates `WinUIEditor/EditorWrapper.cpp`, `.h`, and `.idl`
from `WinUIEditor/WinUIEditor.iface`, `scintilla/include/Scintilla.iface`, and
`lexilla/include/LexicalStyles.iface`. The Lexilla input supplies the existing
style enums; the native component does not compile the Lexilla runtime.
The tool targets .NET 10 / C# 14 and generates CRLF source files.

Run from any working directory:

```powershell
dotnet run --project <native-directory>/Tools/Tool/Tool.csproj -c Release -- Interface
```

The executable finds its native source directory by walking upward from its
own location. If it is copied elsewhere, pass that directory explicitly:

```powershell
dotnet Tool.dll Interface <native-directory>
```

Hand-written APIs live in `WinUIEditor/EditorExtensions.cpp` and
`EditorJournalExtensions.cpp`. Declarations, state and IDL are included through
`EditorExtensions.h`, `EditorExtensionsState.h` and `EditorExtensions.idl.inc`.
Regeneration preserves these extensions. Change generic string marshaling and
notification code in `Tool/InterfaceGeneratorTool.cs`, then regenerate rather
than editing generated methods.

Text-bearing notification arguments borrow native payloads during synchronous
event dispatch. Request `Text` or `TextAsBuffer` then to create an owned copy.
Materialized copies remain readable after dispatch; a late request for an
unmaterialized payload throws `RO_E_CLOSED`. Metadata remains readable.
Use the owned UTF-8 reader and journal APIs for asynchronous file operations.
