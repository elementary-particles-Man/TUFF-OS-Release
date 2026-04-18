# CMake generated Testfile for 
# Source directory: /home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang
# Build directory: /mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/glslang
# 
# This file includes the relevant testing commands required for 
# testing this directory and lists subdirectories to be tested as well.
add_test(glslang-testsuite "bash" "runtests" "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/glslang/localResults" "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/glslang/StandAlone/glslangValidator" "/mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/glslang/StandAlone/spirv-remap")
set_tests_properties(glslang-testsuite PROPERTIES  WORKING_DIRECTORY "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/Test/" _BACKTRACE_TRIPLES "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/CMakeLists.txt;367;add_test;/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/glslang/CMakeLists.txt;0;")
subdirs("External")
subdirs("glslang")
subdirs("OGLCompilersDLL")
subdirs("SPIRV")
subdirs("hlsl")
subdirs("gtests")
