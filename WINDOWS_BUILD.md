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
search path. The simplest approach during development is to copy both DLLs next to the executable:

```
copy C:\vcpkg\installed\x64-windows\bin\ass.dll  target\debug\
copy deps\ffms2\lib\ffms2.dll                     target\debug\
```

For a release build, copy them next to `target\release\samaku.exe` instead.

---

## Code changes

All code changes are either guarded behind platform `cfg` attributes or are pure additions that
do not change existing Linux behaviour.

### `libass-sys/build.rs` — LIBASS_INCLUDE_DIR / LIBASS_LIB_DIR env vars + bindgen target

The original build script relied on pkg-config to find libass. Two changes were made:

1. **Environment variable support** — if `LIBASS_INCLUDE_DIR` is set, it is passed to bindgen as a
   `-I` clang argument; if `LIBASS_LIB_DIR` is set, it is emitted as a `rustc-link-search` directive.
   This is how the paths from `.cargo/config.toml` reach the build script.

2. **bindgen target triple on MSVC Windows** — without this, bindgen (which uses libclang internally)
   parses libass headers using GCC-style `va_list`, generating a `__va_list_tag` type. MSVC defines
   `va_list` as `char*`, so `__va_list_tag` simply does not exist at compile time. Fixed by passing
   the correct target to clang:

   ```rust
   #[cfg(all(target_os = "windows", target_env = "msvc"))]
   {
       builder = builder.clang_arg("--target=x86_64-pc-windows-msvc");
   }
   ```

### `src/media/bindings/ass.rs` — platform-portable libass usage

Three changes were needed due to MSVC generating `i32` for C enum constants whereas the code
assumed `u32` (as generated by bindgen on Linux/GCC):

1. **`VaList` type alias** — the internal libass message callback takes a `va_list` argument. The
   type is platform-dependent:

   ```rust
   #[cfg(not(target_env = "msvc"))]
   type VaList = libass::__va_list_tag;  // GCC: va_list is a struct
   #[cfg(target_env = "msvc")]
   type VaList = i8;                     // MSVC: va_list is char*
   ```

   The callback signature was updated to use `*mut VaList`.

2. **`#[repr(i32)]` for libass-backed enums** — `FontProvider` and `Feature` were `#[repr(u32)]`,
   but on MSVC bindgen generates C enum module consts as `i32`. Changed both to `#[repr(i32)]`.

3. **Cast fixes** — two call sites that cast enum discriminants or struct fields to `u32` before
   passing them to bindgen-generated functions were updated to cast to `i32` instead, since that is
   the type those fields have in the MSVC-generated bindings.
