# yan CLI 安装指南

`yan` 是 yan.chat 平台的终端工具，单二进制零依赖，支持 macOS 和 Linux。

## 快速安装（推荐）

### Shell 一键安装

```bash
curl -fsSL https://raw.githubusercontent.com/yzlabai/yan-pm-cli/main/install.sh | sh
```

自动检测系统和架构，下载最新版本到 `/usr/local/bin/yan`。

### 手动下载

从 [GitHub Releases](https://github.com/yzlabai/yan-pm-cli/releases) 下载对应平台：

| 平台 | 文件 |
|------|------|
| macOS Apple Silicon | `yan-v{VERSION}-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `yan-v{VERSION}-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `yan-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `yan-v{VERSION}-aarch64-unknown-linux-gnu.tar.gz` |

```bash
# 示例：macOS Apple Silicon
curl -LO https://github.com/yzlabai/yan-pm-cli/releases/latest/download/yan-v0.4.0-aarch64-apple-darwin.tar.gz
tar xzf yan-v0.4.0-aarch64-apple-darwin.tar.gz
sudo mv yan-v0.4.0-aarch64-apple-darwin/yan /usr/local/bin/
yan --version
```

## 包管理器安装

### npm / pnpm / yarn

```bash
npm install -g @anthropic/yan-pm    # npm
pnpm add -g @anthropic/yan-pm       # pnpm
yarn global add @anthropic/yan-pm   # yarn
npx @anthropic/yan-pm               # 临时运行
```

> npm 包是一个薄 wrapper，首次运行时自动下载对应平台的二进制。

### Homebrew（macOS / Linux）

```bash
brew tap yzlabai/tap
brew install yan
```

### Cargo（从源码构建）

```bash
cargo install --git https://github.com/yzlabai/yan-pm-cli.git --path crates/yan-pm
```

## 更新

```bash
# 内置自更新（推荐）
yan self-update

# 或通过包管理器
brew upgrade yan
npm update -g @anthropic/yan-pm
```

## 验证安装

```bash
yan --version     # 查看版本
yan --help        # 查看帮助
```

## 首次配置

```bash
# 1. 登录（浏览器授权）
yan --url https://yan.chat login

# 2. 安装到 AI 工具（Claude Code / VS Code / Cursor）
yan setup

# 3. 关联项目目录
cd /path/to/your/repo
yan link <project-slug>
```

配置保存在 `~/.config/yan-pm/config.json`，后续命令自动读取。

## 系统要求

- macOS 12+ (Monterey) 或 Linux (glibc 2.31+)
- 磁盘：~10 MB
- 内存：daemon 模式 < 15 MB

## 卸载

```bash
# 手动安装
sudo rm /usr/local/bin/yan

# Homebrew
brew uninstall yan

# npm
npm uninstall -g @anthropic/yan-pm

# 清理配置（可选）
rm -rf ~/.config/yan-pm
```
