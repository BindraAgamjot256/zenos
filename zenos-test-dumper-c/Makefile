# Makefile for zenos-test-dumper-c
# A C rewrite of zenos-test-dumper-1

CC ?= clang --target=x86_64-unknown-none-elf
LD ?= ld.lld

# Directories
LIBC_DIR := ../libc
LIBC_BUILD := $(LIBC_DIR)/build
BUILD_DIR := build

# Output
TARGET := $(BUILD_DIR)/test-dumper

# Compiler flags for freestanding x86_64
CFLAGS := -std=c11 \
          -Wall -Wextra \
          -ffreestanding \
          -nostdlib \
          -fno-builtin \
          -mno-red-zone \
          -m64 \
          -I$(LIBC_DIR)/include

# Linker flags
LDFLAGS := -static \
           -T link.ld \
           -L$(LIBC_BUILD)

# Default target
.PHONY: all
all: $(TARGET)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Check libc exists
$(LIBC_BUILD)/crt0.o $(LIBC_BUILD)/libc.a:
	@echo "Error: libc not built. Run 'make' in $(LIBC_DIR) first"
	@exit 1

# Compile main.c
$(BUILD_DIR)/main.o: main.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Link the executable
$(TARGET): $(BUILD_DIR)/main.o $(LIBC_BUILD)/crt0.o $(LIBC_BUILD)/libc.a | $(BUILD_DIR)
	$(LD) $(LDFLAGS) $(LIBC_BUILD)/crt0.o $(BUILD_DIR)/main.o -lc -o $@

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: help
help:
	@echo "Usage:"
	@echo "  make        - Build test-dumper"
	@echo "  make clean  - Remove build artifacts"
	@echo ""
	@echo "Prerequisites:"
	@echo "  Run 'make' in ../libc first to build libc"
