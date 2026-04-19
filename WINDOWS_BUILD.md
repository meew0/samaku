# Building samaku on Windows

This document describes the full set of environment setup steps and code changes required to build
samaku on Windows using the MSVC toolchain. The project builds without modification on Linux; all
changes here are additive or conditional on `#[cfg(windows)]` / `#[cfg(target_env = "msvc")]`.

---

## Prerequisites

### 1. Rust (MSVC toolchain)

Install Rust via [rustup](https://rustup.rs/). During installation, or afterwards, set the default
toolchain to the MSVC Windows target:

```
rustup default stable-x86_64-pc-windows-msvc
```

This requires the MSVC C++ build tools to already be installed (see below). The GNU toolchain
(`x86_64-pc-windows-gnu`) will **not** work for this project because the native C++ libraries are
built with MSVC.

### 2. Visual Studio 2022 (MSVC build tools)

Install [Visual Studio 2022](https://visualstudio.microsoft.com/) Community (or higher). During
installation, select the **"Desktop development with C++"** workload. This provides:

- The MSVC compiler (`cl.exe`)
- MSBuild
- The Windows SDK
- The Visual C++ runtime libraries

The exact version tested was **VS 2022 17.11 (MSVC 14.41.34120)**. Newer versions should also work.

You do not need the full IDE; the **"Build Tools for Visual Studio 2022"** standalone package is
sufficient if you prefer a smaller installation.

### 3. CMake

Download and install [CMake](https://cmake.org/download/) (version 3.5 or later). During
installation, choose to add CMake to the system PATH. CMake is required to build the bundled libmv
C++ library.

Verify it is on PATH:
```
cmake --version
```

### 4. libass (via vcpkg)

[vcpkg](https://vcpkg.io/) is the easiest way to get libass and its dependencies on Windows.

Clone and bootstrap vcpkg (the recommended location is `C:\vcpkg`):

```powershell
cd C:\
git clone https://github.com/microsoft/vcpkg.git
cd vcpkg
.\bootstrap-vcpkg.bat
```

**Note on SSL errors during bootstrap:** vcpkg downloads `7-Zip` during bootstrapping. If you
encounter SSL/TLS errors with `curl`, pre-download the required files via PowerShell and place them
in `C:\vcpkg\downloads\`:

```powershell
# Check which files vcpkg tried to download from the bootstrap error output, then:
Invoke-WebRequest -Uri "https://www.7-zip.org/a/7z2600-x64.exe" -OutFile "C:\vcpkg\downloads\7z2600-x64.7z.exe"
Invoke-WebRequest -Uri "https://www.7-zip.org/a/7zr.exe" -OutFile "C:\vcpkg\downloads\a30f8a21-7zr.exe"
```

Then install libass for x64-windows:

```
C:\vcpkg\vcpkg.exe install libass:x64-windows
```

This installs libass and all its dependencies (freetype, fribidi, harfbuzz, etc.) under
`C:\vcpkg\installed\x64-windows\`.

The headers end up at: `C:\vcpkg\installed\x64-windows\include\`
The import libraries at: `C:\vcpkg\installed\x64-windows\lib\`
The runtime DLL at: `C:\vcpkg\installed\x64-windows\bin\ass.dll`

### 5. ffms2

ffms2 provides the video/audio decoding layer. MSVC pre-built binaries are available from the
[ffms2 GitHub releases page](https://github.com/FFMS/ffms2/releases). Download the file named
`ffms2-X.Y-msvc.7z` and extract it.

Place the contents into a `deps/ffms2/` directory inside the repository:

```
deps/
  ffms2/
    include/
      ffms.h
      ffmscompat.h
    lib/
      ffms2.lib      (MSVC import library)
      ffms2.dll      (runtime DLL)
```

The `deps/` directory is already in `.gitignore`.

---

## Environment configuration

Both `.cargo/` and `deps/` are listed in `.gitignore` because they contain machine-specific paths
or binary blobs. Create these locally after cloning.

Create a file `.cargo/config.toml` in the repository root with the following content, adjusting the
paths to match where you extracted ffms2 and where vcpkg is installed:

```toml
[env]
FFMS_INCLUDE_DIR = "C:/path/to/samaku/deps/ffms2/include"
FFMS_LIB_DIR     = "C:/path/to/samaku/deps/ffms2/lib"
LIBASS_INCLUDE_DIR = "C:/vcpkg/installed/x64-windows/include"
LIBASS_LIB_DIR     = "C:/vcpkg/installed/x64-windows/lib"
```

These tell the `ffms2-sys` and `libass-sys` build scripts where to find headers and import
libraries, bypassing the pkg-config discovery that works on Linux but is not available on Windows.

---

## Runtime DLLs

The final binary dynamically loads `ass.dll` and `ffms2.dll` at runtime. These must be on the DLL
search path, *as must be the dependencies of `ass.dll` (present alongside it in the vcpkg `bin` folder)*.

The simplest approach during development is to copy the DLLs next to the executable, e.g.:

```
copy C:\vcpkg\installed\x64-windows\bin\ass.dll target\debug\
copy C:\vcpkg\installed\x64-windows\bin\harfbuzz.dll target\debug\
# and so on...

copy deps\ffms2\lib\ffms2.dll target\debug\
```

(TODO: elaborate on this. ffms2 already gets copied automatically. What else does libass require? Can we copy those automatically as well?)

For a release build, copy them next to `target\release\samaku.exe` instead.
