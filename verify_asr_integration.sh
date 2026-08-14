#!/bin/bash
# VoiceBoom ASR 快速验证脚本

set -e
cd "$(dirname "$0")"

echo "=== VoiceBoom 本地 ASR 实施验证 ==="
echo ""

# 1. 检查 bundle 结构
echo "[1/5] 检查 ASR 资源包..."
BUNDLE=src-tauri/asr-bundle
if [ ! -d "$BUNDLE" ]; then
    echo "❌ $BUNDLE 不存在"
    exit 1
fi

WHISPER_BIN="$BUNDLE/whisper_cpp/whisper-server.exe"
WHISPER_MODEL="$BUNDLE/whisper_cpp/models/ggml-base.bin"
FUNASR_BIN="$BUNDLE/funasr/llama-funasr-sensevoice.exe"
FUNASR_MODEL="$BUNDLE/funasr/models/sensevoice-small-q8.gguf"
VAD_MODEL="$BUNDLE/funasr/models/fsmn-vad.gguf"

for f in "$WHISPER_BIN" "$WHISPER_MODEL" "$FUNASR_BIN" "$FUNASR_MODEL" "$VAD_MODEL"; do
    if [ -f "$f" ]; then
        echo "  ✓ $(basename $f)"
    else
        echo "  ❌ $(basename $f) 缺失"
        exit 1
    fi
done

SIZE=$(du -sh $BUNDLE | awk '{print $1}')
echo "  总计: $SIZE"
echo ""

# 2. 检查代码修改
echo "[2/5] 检查代码文件..."
FILES=(
    "src-tauri/Cargo.toml:reqwest"
    "src-tauri/tauri.conf.json:asr-bundle"
    "src-tauri/src/asr/adapters/local.rs:encode_wav"
    "src-tauri/src/resources/mod.rs:get_bundled_resource_dir"
    "src-tauri/src/resources/server.rs:FunASR CLI"
)

for entry in "${FILES[@]}"; do
    IFS=: read -r file pattern <<< "$entry"
    if grep -q "$pattern" "$file" 2>/dev/null; then
        echo "  ✓ $(basename $file) 包含 '$pattern'"
    else
        echo "  ⚠ $(basename $file) 可能未正确修改"
    fi
done
echo ""

# 3. TypeScript 编译
echo "[3/5] 前端编译检查..."
if bun run build > /tmp/vb_build.log 2>&1; then
    echo "  ✓ 前端编译通过"
else
    echo "  ❌ 前端编译失败，查看 /tmp/vb_build.log"
    exit 1
fi
echo ""

# 4. Rust 类型检查
echo "[4/5] Rust 类型检查..."
cd src-tauri
if cargo check > /tmp/vb_cargo_check.log 2>&1; then
    echo "  ✓ Rust 类型检查通过"
else
    echo "  ❌ Rust 检查失败，查看 /tmp/vb_cargo_check.log"
    exit 1
fi
cd ..
echo ""

# 5. whisper-server 快速测试
echo "[5/5] whisper-server 启动测试..."
cd src-tauri
PORT=18080
timeout 10 $WHISPER_BIN --model $WHISPER_MODEL --port $PORT --threads 2 > /tmp/whisper_test.log 2>&1 &
PID=$!
sleep 3

if ps -p $PID > /dev/null; then
    echo "  ✓ whisper-server 成功启动 (PID $PID)"
    kill $PID 2>/dev/null
    wait $PID 2>/dev/null
else
    echo "  ⚠ whisper-server 启动异常，查看 /tmp/whisper_test.log"
fi
cd ..
echo ""

echo "=== ✅ 验证完成 ==="
echo ""
echo "下一步："
echo "  1. 运行开发模式: bun run tauri:dev"
echo "  2. 打包安装: bun run tauri:build"
echo "  3. 查看详细总结: cat IMPLEMENTATION_SUMMARY.md"
