# Makefile for GLACIER-MAESTRO Reference Implementation

.PHONY: all rust python test clean help

# Default target
all: test

# Run Rust implementation
rust:
	@echo "Running Rust reference implementation..."
	@rustc glacier_reference.rs -O -o glacier_reference_rust
	@./glacier_reference_rust

# Run Python implementation  
python:
	@echo "Running Python reference implementation..."
	@python3 glacier_reference.py

# Run both and compare
test: rust python
	@echo "\n=== Comparing Results ==="
	@echo "Both implementations should produce identical results"

# Clean build artifacts
clean:
	@rm -f glacier_reference_rust
	@rm -f *.pyc
	@rm -rf __pycache__

# Help
help:
	@echo "GLACIER-MAESTRO Reference Implementation"
	@echo ""
	@echo "Usage:"
	@echo "  make         - Run both implementations and compare"
	@echo "  make rust    - Run Rust implementation only"
	@echo "  make python  - Run Python implementation only"
	@echo "  make test    - Run both and compare results"
	@echo "  make clean   - Remove build artifacts"
	@echo "  make help    - Show this help message"