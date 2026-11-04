@echo off
rem Copyright 2026 by Breece Walker
rem See src/Notepads.Native/LICENSE for the original WinUIEdit license.
rem
rem Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
rem See LICENSE.txt in the project root for the Notepads modifications.


dotnet run --project "%~dp0Tool\Tool.csproj" -c Release -- Interface
