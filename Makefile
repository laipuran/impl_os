.DEFAULT_GOAL := all

CARGO ?= cargo
MODE ?= release
FEATURES ?=
TARGET_DIR := $(CURDIR)/target

ifeq ($(MODE),release)
PROFILE_ARG := --release
else ifeq ($(MODE),debug)
PROFILE_ARG :=
else
$(error MODE must be release or debug)
endif

.PHONY: all kernel-rv kernel-la clean

# Contest entry point: always build both ELF artifacts, even if copies exist.
all: kernel-rv kernel-la

kernel-rv:
	$(CARGO) build --locked --offline -p impl-os --target riscv64gc-unknown-none-elf --target-dir "$(TARGET_DIR)" $(PROFILE_ARG) $(if $(strip $(FEATURES)),--features "$(FEATURES)")
	cp -- "$(TARGET_DIR)/riscv64gc-unknown-none-elf/$(MODE)/impl-os" "$@"

kernel-la:
	$(CARGO) build --locked --offline -p impl-os --target loongarch64-unknown-none --target-dir "$(TARGET_DIR)" $(PROFILE_ARG) $(if $(strip $(FEATURES)),--features "$(FEATURES)")
	cp -- "$(TARGET_DIR)/loongarch64-unknown-none/$(MODE)/impl-os" "$@"

clean:
	$(CARGO) clean --target-dir "$(TARGET_DIR)"
	rm -f kernel-rv kernel-la
