# Install script for directory: /home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV

# Set the install prefix
if(NOT DEFINED CMAKE_INSTALL_PREFIX)
  set(CMAKE_INSTALL_PREFIX "/media/flux/THPDOC/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/debug/build/shaderc-sys-c8131f28372ad02a/out")
endif()
string(REGEX REPLACE "/$" "" CMAKE_INSTALL_PREFIX "${CMAKE_INSTALL_PREFIX}")

# Set the install configuration name.
if(NOT DEFINED CMAKE_INSTALL_CONFIG_NAME)
  if(BUILD_TYPE)
    string(REGEX REPLACE "^[^A-Za-z0-9_]+" ""
           CMAKE_INSTALL_CONFIG_NAME "${BUILD_TYPE}")
  else()
    set(CMAKE_INSTALL_CONFIG_NAME "Release")
  endif()
  message(STATUS "Install configuration: \"${CMAKE_INSTALL_CONFIG_NAME}\"")
endif()

# Set the component getting installed.
if(NOT CMAKE_INSTALL_COMPONENT)
  if(COMPONENT)
    message(STATUS "Install component: \"${COMPONENT}\"")
    set(CMAKE_INSTALL_COMPONENT "${COMPONENT}")
  else()
    set(CMAKE_INSTALL_COMPONENT)
  endif()
endif()

# Install shared libraries without execute permission?
if(NOT DEFINED CMAKE_INSTALL_SO_NO_EXE)
  set(CMAKE_INSTALL_SO_NO_EXE "1")
endif()

# Is this installation the result of a crosscompile?
if(NOT DEFINED CMAKE_CROSSCOMPILING)
  set(CMAKE_CROSSCOMPILING "FALSE")
endif()

# Set path to fallback-tool for dependency-resolution.
if(NOT DEFINED CMAKE_OBJDUMP)
  set(CMAKE_OBJDUMP "/usr/bin/objdump")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib" TYPE STATIC_LIBRARY FILES "/media/flux/THPDOC/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/debug/build/shaderc-sys-c8131f28372ad02a/out/build/glslang/SPIRV/libSPIRV.a")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/cmake" TYPE FILE FILES "/media/flux/THPDOC/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/debug/build/shaderc-sys-c8131f28372ad02a/out/build/glslang/SPIRV/SPIRVTargets.cmake")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/include/glslang/SPIRV" TYPE FILE FILES
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/bitutils.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/spirv.hpp"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.std.450.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.ext.EXT.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.ext.KHR.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GlslangToSpv.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/hex_float.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/Logger.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/SpvBuilder.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/spvIR.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/doc.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/SpvTools.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/disassemble.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.ext.AMD.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.ext.NV.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/GLSL.ext.ARM.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/NonSemanticDebugPrintf.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/NonSemanticShaderDebugInfo100.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/SPVRemapper.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/SPIRV/doc.h"
    )
endif()

string(REPLACE ";" "\n" CMAKE_INSTALL_MANIFEST_CONTENT
       "${CMAKE_INSTALL_MANIFEST_FILES}")
if(CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/media/flux/THPDOC/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/debug/build/shaderc-sys-c8131f28372ad02a/out/build/glslang/SPIRV/install_local_manifest.txt"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
