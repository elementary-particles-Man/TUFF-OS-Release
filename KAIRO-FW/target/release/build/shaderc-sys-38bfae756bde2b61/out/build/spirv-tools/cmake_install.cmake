# Install script for directory: /home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools

# Set the install prefix
if(NOT DEFINED CMAKE_INSTALL_PREFIX)
  set(CMAKE_INSTALL_PREFIX "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out")
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

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/external/cmake_install.cmake")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/source/cmake_install.cmake")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/tools/cmake_install.cmake")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/test/cmake_install.cmake")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/examples/cmake_install.cmake")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/include/spirv-tools" TYPE FILE FILES
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools/include/spirv-tools/libspirv.h"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools/include/spirv-tools/libspirv.hpp"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools/include/spirv-tools/optimizer.hpp"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools/include/spirv-tools/linker.hpp"
    "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/spirv-tools/include/spirv-tools/instrument.hpp"
    )
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/pkgconfig" TYPE FILE FILES
    "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/SPIRV-Tools.pc"
    "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/SPIRV-Tools-shared.pc"
    )
endif()

string(REPLACE ";" "\n" CMAKE_INSTALL_MANIFEST_CONTENT
       "${CMAKE_INSTALL_MANIFEST_FILES}")
if(CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/spirv-tools/install_local_manifest.txt"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
