# TODO / 技术债

记录已识别但暂缓的改进项，避免遗忘。

---

## 1. 函数「声明 → 定义」解析（借鉴 clangd）

### 问题
`locate` / `resolve_symbol` 对**函数**可能选到声明（原型 `void Foo();`）而非定义。
class/struct 的前向声明已在扫描器修复（commit `e1fa476`），但函数原型仍未区分。

### clangd 是怎么做的（参考）
1. **USR（Unified Symbol Resolution）**：Clang 给每个符号生成语义指纹字符串，
   编码「完全限定名 + 签名类型 + 模板参数」。同一函数的声明与定义 **USR 完全相同**，
   重载 `foo(int)` / `foo(double)` USR 不同 —— 关联靠 USR 相等，不靠名字。
2. **索引分字段存**：每个 Symbol 同时记 `CanonicalDeclaration` 与 `Definition` 两个位置，
   不是二选一。
3. **跳转策略**：引用 → 解析出 USR → 查索引 → **优先返回 Definition，没有才退回 Declaration**。
4. **definition 判定**：AST `FunctionDecl::isThisDeclarationADefinition()` —— 有函数体即定义。

### 红线约束下的借鉴方案（不使用编译器/AST）
- **数据模型**：symbols 表加 `role` 列（`definition` | `declaration`），decl/def **都记、打标**，
  不再丢弃声明（学 clangd 分字段思想）。
- **definition 判定（词法近似）**：有函数体 `{` 即定义；以 `;` 收尾即声明。
  等价 clangd 的 `isThisDeclarationADefinition()` 的词法版。
- **关键：信号已存在**：`scanner/common.rs::scan_scoped_calls` 调用的
  `signature_of(line) -> (name, same_line_body)` 中的 `same_line_body`（及 pending 下一行 `{`）
  **已经在判定函数是否有 body**，目前只喂给 call 边，未回喂符号表。
- **resolve 优先 definition**：`resolve_symbol` / `locate` 排序加最高优先维度
  `ORDER BY (role='definition') DESC, ...`，等价 clangd「有 def 用 def」。
- **弱指纹（近似 USR）**：用「限定名 `Class::method` + 参数个数（括号内逗号数）」当关联键，
  比纯名字强，但**同名不同参类型的重载仍分不清**（无编译器的天花板）。

### 落地拦路石（如实标注）
- `scan_scoped_calls`（花括号状态机，知道 body）与 `symbol_of`（逐行提取符号）是**两条独立遍历**。
  给符号打 role 需共享「当前行是否开函数体」信号 —— 要么合并两条遍历（干净但改动大），
  要么让 `symbol_of` 做弱判定（`)` 结尾且无 `;` → 定义头，快但对多行签名不准）。
- **多行签名死角**：`void Foo::Bar(\n int a)\n{` 签名行看不到后面的 `{`，词法方案会漏判。
  clangd 靠 AST 无此问题。
- **天花板**：无编译器 → 重载（同名、参数类型不同）无法区分，只能到「参数个数」粒度。

### 最小可行版（MVP）
1. symbols 加 `role` 列。
2. 函数提取用「`)` 结尾且无 `;`」近似打 `definition`，否则 `declaration`。
3. `resolve_symbol` / `locate` 加 `role='definition' DESC` 优先。
4. 多行签名死角与重载天花板如实标注，不假装解决。
