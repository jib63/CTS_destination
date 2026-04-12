@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
echo LIB=%LIB%
set CARGO_HTTP_CHECK_REVOKE=false
cd /d "C:\Dev\Rust\CTS"
cargo build 2>&1
