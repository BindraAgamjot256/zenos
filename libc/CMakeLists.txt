cmake_minimum_required(VERSION 4.1)
project(libc C)

# Force C11 standard
set(CMAKE_C_STANDARD 11)

# Ensure x86_64 only
if (NOT CMAKE_SYSTEM_PROCESSOR STREQUAL "x86_64")
    message(FATAL_ERROR "This project only supports x86_64 architecture.")
endif ()

# Include directories
include_directories(
        .
        include
        include/sys
        src
)

# Sources
set(SOURCES
        include/sys/syscall.h
        include/sys/types.h
        include/fcntl.h
        include/stdio.h
        include/string.h
        include/unistd.h
        src/fcntl.c
        src/stdio.c
        src/string.c
        src/syscall.c
        src/unistd.c
        include/stddef.h
        include/stdarg.h
)

# Build as a static library
add_library(libc STATIC ${SOURCES})

# Optional: make sure compiler uses -m64
set_target_properties(libc PROPERTIES COMPILE_FLAGS "-m64")
