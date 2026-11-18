@echo off
rem Builds kojic, the Koji compiler, on Windows (x86-64) from the seed: an executable of the
rem compiler, bootstrap\seed\kojic-windows-x64.gz, as bootstrap/build.sh does elsewhere. Needs
rem PowerShell (to unpack the seed) only: the seed builds the runtime library and the compiler of
rem compiler\, machine code and linking included.
rem
rem     bootstrap\build.cmd    builds build\kojic.exe and build\libkoji_runtime.a
setlocal
cd /d "%~dp0\.."
if not exist build mkdir build

echo unpacking the seed for windows-x64
powershell -NoProfile -Command "$in = [IO.File]::OpenRead('bootstrap\seed\kojic-windows-x64.gz'); $gz = New-Object IO.Compression.GzipStream($in, [IO.Compression.CompressionMode]::Decompress); $out = [IO.File]::Create('build\stage1.exe'); $gz.CopyTo($out); $out.Close(); $gz.Close()" || exit /b 1

echo building the runtime with the seed
build\stage1.exe library -o build\libkoji_stage1.a || exit /b 1

echo building kojic from compiler\
set KOJI_RUNTIME=%CD%\build\libkoji_stage1.a
build\stage1.exe build compiler/main.kj -o build\kojic.exe || exit /b 1
set KOJI_RUNTIME=

echo building the runtime with kojic
build\kojic.exe library -o build\libkoji_runtime.a || exit /b 1

echo done: build\kojic.exe, with build\libkoji_runtime.a next to it
