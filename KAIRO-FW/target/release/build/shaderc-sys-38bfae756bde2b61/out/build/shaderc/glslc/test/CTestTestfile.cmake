# CMake generated Testfile for 
# Source directory: /home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test
# Build directory: /mnt/efi_test/Develop/TUFF-OS/TUFF-KAIRO/KAIRO-APP/target/release/build/shaderc-sys-38bfae756bde2b61/out/build/shaderc/glslc/test
# 
# This file includes the relevant testing commands required for 
# testing this directory and lists subdirectories to be tested as well.
add_test(shaderc_expect_unittests "/usr/bin/python3" "-m" "unittest" "expect_unittest.py")
set_tests_properties(shaderc_expect_unittests PROPERTIES  WORKING_DIRECTORY "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test" _BACKTRACE_TRIPLES "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test/CMakeLists.txt;15;add_test;/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test/CMakeLists.txt;0;")
add_test(shaderc_glslc_test_framework_unittests "/usr/bin/python3" "-m" "unittest" "glslc_test_framework_unittest.py")
set_tests_properties(shaderc_glslc_test_framework_unittests PROPERTIES  WORKING_DIRECTORY "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test" _BACKTRACE_TRIPLES "/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test/CMakeLists.txt;18;add_test;/home/flux/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/shaderc-sys-0.8.3/build/shaderc/glslc/test/CMakeLists.txt;0;")
