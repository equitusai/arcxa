#!/bin/bash
# Build script for graphica-model-service
# This script works around Anaconda conda environment issues

set -e

echo "Building graphica-model-service..."
echo "Note: This build uses ort with load-dynamic feature"
echo "You'll need to set ORT_DYLIB_PATH at runtime to point to libonnxruntime.so"

# Clear conda environment variables that conflict with system libraries
env -u CC \
    -u CFLAGS \
    -u CPPFLAGS \
    -u LDFLAGS \
    -u C_INCLUDE_PATH \
    -u CPLUS_INCLUDE_PATH \
    -u LIBRARY_PATH \
    PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
    RUSTFLAGS="" \
    cargo build "$@"

echo ""
echo "Build complete!"
echo ""
echo "IMPORTANT: Before running the service, set the ONNX Runtime library path:"
echo "  export ORT_DYLIB_PATH=/path/to/libonnxruntime.so"
echo "  Or initialize it in code with: ort::init_from(dylib_path).commit()"
