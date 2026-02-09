# Makefile for zenos-stress-tests
# Builds stress test programs for Zenos kernel

CC ?= clang --target=x86_64-unknown-none-elf
LD ?= ld.lld

# Directories
LIBC_DIR := ../libc
LIBC_BUILD := $(LIBC_DIR)/build
BUILD_DIR := build

# All test sources
SOURCES := fork_storm.c \
           rapid_spawn.c \
           sched_fairness.c \
           mem_exhaust.c \
           orphan_zombie.c \
           ansi_colors.c

# Derive targets from sources
TARGETS := $(patsubst %.c,$(BUILD_DIR)/%,$(SOURCES))

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
all: $(TARGETS)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Check libc exists
$(LIBC_BUILD)/crt0.o $(LIBC_BUILD)/libc.a:
	@echo "Error: libc not built. Run 'make' in $(LIBC_DIR) first"
	@exit 1

# Pattern rule: compile .c to .o
$(BUILD_DIR)/%.o: %.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Pattern rule: link .o to executable
$(BUILD_DIR)/%: $(BUILD_DIR)/%.o $(LIBC_BUILD)/crt0.o $(LIBC_BUILD)/libc.a | $(BUILD_DIR)
	$(LD) $(LDFLAGS) $(LIBC_BUILD)/crt0.o $< -lc -o $@

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: list
list:
	@echo "Stress tests:"
	@for t in $(SOURCES:.c=); do echo "  $$t"; done

.PHONY: help
help:
	@echo "Usage:"
	@echo "  make        - Build all stress tests"
	@echo "  make clean  - Remove build artifacts"
	@echo "  make list   - List available tests"
	@echo ""
	@echo "Prerequisites:"
	@echo "  Run 'make' in ../libc first to build libc"
	@echo ""
	@echo "Tests:"
	@echo "  fork_storm     - Controlled fork bomb"
	@echo "  rapid_spawn    - Fast process create/destroy"
	@echo "  sched_fairness - Scheduler fairness test"
	@echo "  mem_exhaust    - Memory exhaustion"
	@echo "  orphan_zombie  - Orphan/zombie handling"
	@echo "  fs_concurrent  - Concurrent filesystem access"
	@echo "  procfs_churn   - Procfs stress during churn"
	@echo "  syscall_abuse  - Invalid syscall arguments"
