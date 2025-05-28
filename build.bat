@echo off

set vswhere="%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set vcvarsLookup=call %vswhere% -latest -property installationPath

for /f "tokens=*" %%i in ('%vcvarsLookup%') do set vcvars="%%i\VC\Auxiliary\Build\vcvars64.bat"
call %vcvars%

cargo nextest run -p "pallet-*" -p gear-lazy-pages -p gear-runtime-interface
