# Makefile for libc (freestanding, x86_64)
# Usage:
#   make              - build with clang (default)
#   make CC=gcc       - build with gcc
#   make clean        - remove build artifacts
#
# For cross-compilation from non-x86_64 hosts:
#   make CC=x86_64-elf-gcc AR=x86_64-elf-ar
#   make CC="clang --target=x86_64-unknown-none-elf"
#
# Note: This library uses x86_64 inline assembly and must be
# compiled for x86_64 target (native or cross-compiled).
#
# Output:
#   build/libc.a  - static library
#   build/crt0.o  - C runtime startup object (link first!)
#   build/crt1.o  - C runtime initialization (linked into libc.a)

# Default compiler is clang
CC ?= clang
AR ?= ar
NASM ?= nasm

# Target architecture
ARCH := x86_64

# Directories
SRC_DIR := src
INC_DIR := include
BUILD_DIR := build

# Output
LIB_NAME := libc.a
CRT0_OBJ := $(BUILD_DIR)/crt0.o
OUTPUT := $(BUILD_DIR)/$(LIB_NAME)

# Compiler flags for freestanding x86_64
CFLAGS := -std=c11 \
          -Wall -Wextra \
          -ffreestanding \
          -nostdlib \
          -fno-builtin \
          -mno-red-zone \
          -m64 \
          -I$(INC_DIR) \
          -I$(INC_DIR)/sys \
          -I$(SRC_DIR)

# NASM flags for x86_64 ELF
NASMFLAGS := -f elf64

# Add optimization unless DEBUG is set
ifndef DEBUG
CFLAGS += -O3
else
CFLAGS += -O2 -g
NASMFLAGS += -g -F dwarf
endif

# Source files (exclude crt0.asm from C sources)
SRCS := $(wildcard $(SRC_DIR)/*.c)
OBJS := $(patsubst $(SRC_DIR)/%.c,$(BUILD_DIR)/%.o,$(SRCS))

# Default target
.PHONY: all
all: $(CRT0_OBJ) $(OUTPUT)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Assemble crt0
$(CRT0_OBJ): $(SRC_DIR)/crt0.asm | $(BUILD_DIR)
	$(NASM) $(NASMFLAGS) $< -o $@

# Compile C sources
$(BUILD_DIR)/%.o: $(SRC_DIR)/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Create static library (includes crt1.o but NOT crt0.o)
$(OUTPUT): $(OBJS)
	$(AR) rcs $@ $^
	@echo "Built $(OUTPUT) with $(CC)"
	@echo "Built $(CRT0_OBJ)"
	@echo ""
	@echo "Link order: $(CRT0_OBJ) <your_objects> -L$(BUILD_DIR) -lc"

# Clean build artifacts
.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

# Show configuration
.PHONY: info
info:
	@echo "CC:       $(CC)"
	@echo "NASM:     $(NASM)"
	@echo "ARCH:     $(ARCH)"
	@echo "CFLAGS:   $(CFLAGS)"
	@echo "SRCS:     $(SRCS)"
	@echo "OUTPUT:   $(OUTPUT)"
	@echo "CRT0:     $(CRT0_OBJ)"

.PHONY: help
help:
	@echo "Usage:"
	@echo "  make              - Build with clang (default)"
	@echo "  make CC=gcc       - Build with gcc"
	@echo "  make DEBUG=1      - Build with debug symbols"
	@echo "  make clean        - Remove build artifacts"
	@echo "  make info         - Show build configuration"
	@echo ""
	@echo "Output files:"
	@echo "  build/crt0.o      - C runtime startup (link FIRST)"
	@echo "  build/libc.a      - Static library (includes crt1.o)"
