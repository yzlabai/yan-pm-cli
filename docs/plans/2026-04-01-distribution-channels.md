# yan CLI 分发渠道实施方案

> 日期：2026-04-01 | 目标：让用户通过 npm/brew/shell 一键安装 yan CLI

## 现状

- 二进制名：`yan`，版本 v0.4.0
- 4 平台：macOS arm64/x64 + Linux arm64/x64
- GitHub Actions 自动构建 → tar.gz → GitHub Releases
- 安装方式：手动下载 或 `cargo install`

## 方案概览

| 渠道 | 用户命令 | 优先级 | 复杂度 |
|------|---------|--------|--------|
| **Shell 安装脚本** | `curl ... \| sh` | P0 | 低 |
| **npm** | `npm i -g @yzlab/yan` | P0 | 中 |
| **Homebrew** | `brew install yzlabai/tap/yan` | P1 | 中 |

---

## P0: Shell 安装脚本

新建 `install.sh` 放到仓库根目录。

**逻辑**：检测 OS + ARCH → 拼接 GitHub Release URL → 下载 → 解压 → 移动到 `/usr/local/bin`。

```bash
#!/bin/sh
set -e

REPO="yzlabai/yan-pm-cli"
BINARY="yan"
INSTALL_DIR="/usr/local/bin"

# 检测平台
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux)  OS="unknown-linux-gnu" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *)             echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"

# 获取最新版本
VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"v\(.*\)".*/\1/')
FILENAME="${BINARY}-v${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/v${VERSION}/${FILENAME}"

echo "Installing yan v${VERSION} (${TARGET})..."
TMP=$(mktemp -d)
curl -fsSL "$URL" -o "$TMP/$FILENAME"
tar xzf "$TMP/$FILENAME" -C "$TMP"
sudo install -m 755 "$TMP/${BINARY}-v${VERSION}-${TARGET}/${BINARY}" "$INSTALL_DIR/${BINARY}"
rm -rf "$TMP"
echo "yan v${VERSION} installed to ${INSTALL_DIR}/${BINARY}"
```

**Release workflow 补充**：在 `publish-release` job 后不需要额外操作，脚本直接读 GitHub API。

---

## P0: npm 分发（optionalDependencies 模式）

参考 biome / esbuild 的工业标准模式。

### 包结构

```
npm/
  yan/                          # 主包 @yzlab/yan
    package.json
    bin/yan.js                  # 薄 wrapper
  yan-darwin-arm64/             # @yzlab/yan-darwin-arm64
    package.json
    bin/yan                     # 二进制
  yan-darwin-x64/               # @yzlab/yan-darwin-x64
    package.json
    bin/yan
  yan-linux-x64/                # @yzlab/yan-linux-x64
    package.json
    bin/yan
  yan-linux-arm64/              # @yzlab/yan-linux-arm64
    package.json
    bin/yan
```

### 主包 `@yzlab/yan/package.json`

```json
{
  "name": "@yzlab/yan",
  "version": "0.4.0",
  "description": "yan.chat CLI — project management + AI agent execution",
  "bin": { "yan": "bin/yan.js" },
  "files": ["bin/yan.js"],
  "optionalDependencies": {
    "@yzlab/yan-darwin-arm64": "0.4.0",
    "@yzlab/yan-darwin-x64": "0.4.0",
    "@yzlab/yan-linux-x64": "0.4.0",
    "@yzlab/yan-linux-arm64": "0.4.0"
  },
  "license": "MIT",
  "repository": "https://github.com/yzlabai/yan-pm-cli"
}
```

### 平台包 `@yzlab/yan-darwin-arm64/package.json`

```json
{
  "name": "@yzlab/yan-darwin-arm64",
  "version": "0.4.0",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "files": ["bin/yan"],
  "license": "MIT"
}
```

### Wrapper `bin/yan.js`

```js
#!/usr/bin/env node
const { platform, arch } = process;
const { spawnSync } = require("child_process");

const PLATFORMS = {
  darwin: { arm64: "@yzlab/yan-darwin-arm64", x64: "@yzlab/yan-darwin-x64" },
  linux:  { arm64: "@yzlab/yan-linux-arm64",  x64: "@yzlab/yan-linux-x64" },
};

const pkg = PLATFORMS?.[platform]?.[arch];
if (!pkg) {
  console.error(`Unsupported platform: ${platform}-${arch}`);
  process.exit(1);
}

let binPath;
try {
  binPath = require.resolve(`${pkg}/bin/yan`);
} catch {
  console.error(`Missing platform package: ${pkg}\nRun: npm install`);
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: "inherit",
  shell: false,
});
process.exitCode = result.status ?? 1;
```

### Release workflow 补充

在 `.github/workflows/release.yml` 的 `publish-release` job 后新增：

```yaml
  publish-npm:
    name: Publish to npm
    needs: publish-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          registry-url: https://registry.npmjs.org

      - name: Download release artifacts
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          tag="${GITHUB_REF#refs/tags/}"
          version="${tag#v}"
          mkdir -p /tmp/artifacts

          for target in x86_64-apple-darwin aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
            gh release download "$tag" -p "yan-${tag}-${target}.tar.gz" -D /tmp/artifacts
            mkdir -p "/tmp/artifacts/${target}"
            tar xzf "/tmp/artifacts/yan-${tag}-${target}.tar.gz" -C "/tmp/artifacts/${target}" --strip-components=1
          done

      - name: Build and publish npm packages
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: |
          tag="${GITHUB_REF#refs/tags/}"
          version="${tag#v}"

          # 平台映射
          declare -A PKG_MAP=(
            ["aarch64-apple-darwin"]="yan-darwin-arm64"
            ["x86_64-apple-darwin"]="yan-darwin-x64"
            ["x86_64-unknown-linux-gnu"]="yan-linux-x64"
            ["aarch64-unknown-linux-gnu"]="yan-linux-arm64"
          )
          declare -A OS_MAP=(
            ["yan-darwin-arm64"]="darwin"
            ["yan-darwin-x64"]="darwin"
            ["yan-linux-x64"]="linux"
            ["yan-linux-arm64"]="linux"
          )
          declare -A CPU_MAP=(
            ["yan-darwin-arm64"]="arm64"
            ["yan-darwin-x64"]="x64"
            ["yan-linux-x64"]="x64"
            ["yan-linux-arm64"]="arm64"
          )

          # 发布平台包
          for target in "${!PKG_MAP[@]}"; do
            pkg="${PKG_MAP[$target]}"
            dir="/tmp/npm-${pkg}"
            mkdir -p "$dir/bin"
            cp "/tmp/artifacts/${target}/yan" "$dir/bin/yan"
            chmod +x "$dir/bin/yan"
            cat > "$dir/package.json" << EOF
          {
            "name": "@yzlab/${pkg}",
            "version": "${version}",
            "os": ["${OS_MAP[$pkg]}"],
            "cpu": ["${CPU_MAP[$pkg]}"],
            "files": ["bin/yan"],
            "license": "MIT"
          }
          EOF
            cd "$dir" && npm publish --access public
          done

          # 发布主包
          dir="/tmp/npm-yan"
          mkdir -p "$dir/bin"
          cp npm/yan/bin/yan.js "$dir/bin/yan.js"
          cat > "$dir/package.json" << EOF
          {
            "name": "@yzlab/yan",
            "version": "${version}",
            "bin": { "yan": "bin/yan.js" },
            "files": ["bin/yan.js"],
            "optionalDependencies": {
              "@yzlab/yan-darwin-arm64": "${version}",
              "@yzlab/yan-darwin-x64": "${version}",
              "@yzlab/yan-linux-x64": "${version}",
              "@yzlab/yan-linux-arm64": "${version}"
            },
            "license": "MIT"
          }
          EOF
            cd "$dir" && npm publish --access public
```

---

## P1: Homebrew Tap

### 创建 tap 仓库

新建 GitHub 仓库 `yzlabai/homebrew-tap`，结构：

```
homebrew-tap/
  Formula/yan.rb
  .github/workflows/update.yml
```

### Formula (`Formula/yan.rb`)

```ruby
class Yan < Formula
  desc "yan.chat CLI — project management + AI agent execution"
  homepage "https://github.com/yzlabai/yan-pm-cli"
  version "0.4.0"
  license "MIT"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/yzlabai/yan-pm-cli/releases/download/v#{version}/yan-v#{version}-aarch64-apple-darwin.tar.gz"
    sha256 "SHA256_PLACEHOLDER"
  elsif OS.mac? && Hardware::CPU.intel?
    url "https://github.com/yzlabai/yan-pm-cli/releases/download/v#{version}/yan-v#{version}-x86_64-apple-darwin.tar.gz"
    sha256 "SHA256_PLACEHOLDER"
  elsif OS.linux? && Hardware::CPU.arm?
    url "https://github.com/yzlabai/yan-pm-cli/releases/download/v#{version}/yan-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
    sha256 "SHA256_PLACEHOLDER"
  elsif OS.linux? && Hardware::CPU.intel?
    url "https://github.com/yzlabai/yan-pm-cli/releases/download/v#{version}/yan-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "SHA256_PLACEHOLDER"
  end

  def install
    bin.install "yan"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/yan --version")
  end
end
```

### 自动更新 workflow

Release workflow 新增 job，用 `repository_dispatch` 触发 tap 仓库更新：

```yaml
# 在 yan-pm-cli release.yml 末尾
  update-homebrew:
    needs: publish-release
    runs-on: ubuntu-latest
    steps:
      - uses: peter-evans/repository-dispatch@v3
        with:
          token: ${{ secrets.TAP_GITHUB_TOKEN }}
          repository: yzlabai/homebrew-tap
          event-type: update-tap
          client-payload: '{"version": "${{ github.ref_name }}"}'
```

tap 仓库的 workflow 接收事件后：下载 4 个 tar.gz → 计算 sha256 → 重写 `Formula/yan.rb` → commit push。

---

## 注意事项

### tar.gz 内部结构

当前 release 打包是 `yan-v0.4.0-target/yan`（嵌套一层目录）。brew `bin.install "yan"` 和 npm 需要二进制在解压后直接可用。两种方案：

1. **改 release workflow**：打包时不嵌套目录（`tar czf ... yan README.md`）
2. **安装脚本适配**：`tar xzf ... --strip-components=1`

建议方案 1，改一次受益所有渠道。

### npm scope

需要在 npmjs.com 注册 `@yzlab` scope（组织）。或者用非 scope 包名 `yan-cli`（如果 `yan` 已被占用）。

### Homebrew tap token

release workflow 需要一个对 `yzlabai/homebrew-tap` 有写权限的 PAT，存为 `TAP_GITHUB_TOKEN` secret。

---

## 实施顺序

1. **改 release tar.gz 结构**（去掉嵌套目录）
2. **写 `install.sh`** + 更新 README
3. **创建 npm 包结构** + wrapper 脚本 + CI publish job
4. **创建 `homebrew-tap` 仓库** + formula + 自动更新
5. **README 更新**所有安装方式
