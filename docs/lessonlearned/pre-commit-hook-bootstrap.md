# Pre-commit Hook 引入时的注意事项

## 问题

给 yan-pm-cli 添加 `.githooks/pre-commit`（cargo fmt + clippy + test），首次提交就被自己的 hook 拦住：
- 13 个 clippy 警告（unused imports、dead code、useless format）
- 多处格式不一致
- 修完 clippy 后 fmt 又报新差异（链式调用换行）

连续 3 次提交失败才最终通过。

## 原则

**先还清存量债务，再开启严格门禁。** 引入 `-D warnings` 级别的 lint gate 前，必须确保现有代码已经 clean。

## 做法

1. 先运行一次 `cargo fmt && cargo clippy --all-targets -- -D warnings`，修掉所有问题
2. 修完后再次 `cargo fmt`（clippy 修复可能引入新的格式差异）
3. 确认 clean 后再添加 hook 并一起提交
4. 对暂未使用但属于公共 API 的项，用 `#[allow(dead_code)]` 标注而非删除
