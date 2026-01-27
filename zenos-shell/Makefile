# Makefile for zenos-shell
# A basic shell for Zenos kernel

CC ?= clang --target=x86_64-unknown-none-elf
LD ?= ld.lld

# Directories
LIBC_DIR := ../libc
LIBC_BUILD := $(LIBC_DIR)/build
SRC_DIR := src
BUILD_DIR := build

# Output
TARGET := $(BUILD_DIR)/shell

# Source files
SRCS := $(wildcard $(SRC_DIR)/*.c)
OBJS := $(patsubst $(SRC_DIR)/%.c,$(BUILD_DIR)/%.o,$(SRCS))

# Compiler flags for freestanding x86_64
CFLAGS := -std=c11 \
          -Wall -Wextra \
          -ffreestanding \
          -nostdlib \
          -fno-builtin \
          -mno-red-zone \
          -m64 \
          -I$(LIBC_DIR)/include \
          -I$(SRC_DIR)

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

# Compile source files
$(BUILD_DIR)/%.o: $(SRC_DIR)/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Link the executable
$(TARGET): $(OBJS) $(LIBC_BUILD)/crt0.o $(LIBC_BUILD)/libc.a | $(BUILD_DIR)
	$(LD) $(LDFLAGS) $(LIBC_BUILD)/crt0.o $(OBJS) -lc -o $@

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: help
help:
	@echo "Usage:"
	@echo "  make        - Build shell"
	@echo "  make clean  - Remove build artifacts"
	@echo ""
	@echo "Source files: $(SRCS)"
