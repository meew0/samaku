project('BestSourceW', 'cpp',
    default_options: ['buildtype=release', 'b_lto=true', 'b_ndebug=if-release', 'cpp_std=c++14'],
    license: 'MIT',
    meson_version: '>=0.51.0',
    version: '0.9'
)

sources = [
    'bestsource/src/audiosource.cpp',
    'bestsource/src/videosource.cpp',
    'bestsource/src/SrcAttribCache.cpp',
    'bestsource/src/BSRational.cpp',
    'bestsource/src/vapoursynth.cpp',
    'wrapper/wrapper.cpp'
]

libs = []
p2p_args = []

if host_machine.cpu_family().startswith('x86')
    p2p_args += ['-DP2P_SIMD']
endif

libs += static_library('p2p_main',
    [
        'libp2p/p2p_api.cpp',
        'libp2p/v210.cpp',
        'libp2p/simd/cpuinfo_x86.cpp',
        'libp2p/simd/p2p_simd.cpp'
    ],
    cpp_args: p2p_args,
    gnu_symbol_visibility: 'hidden'
)

if host_machine.cpu_family().startswith('x86')
    p2p_args += ['-msse4.1']

    libs += static_library('p2p_sse41', 'libp2p/simd/p2p_sse41.cpp',
        cpp_args: p2p_args,
        gnu_symbol_visibility: 'hidden'
    )
endif

vapoursynth_dep = dependency('vapoursynth', version: '>=55').partial_dependency(compile_args: true, includes: true)

deps = [
    vapoursynth_dep,
    dependency('jansson', version: '>=2.12', static: false),
    dependency('libavcodec', version: '>=59.37.0', static: false),
    dependency('libavformat', version: '>=59.27.0', static: false),
    dependency('libavutil', version: '>=57.28.0', static: false),
    dependency('libswscale', version: '>=6.7.0', static: false)
]

shared_module('bestsourcew', sources,
    dependencies: deps,
    gnu_symbol_visibility: 'default',
    install: true,
    install_dir: vapoursynth_dep.get_variable(pkgconfig: 'libdir') / 'vapoursynth',
    link_with: libs
)
