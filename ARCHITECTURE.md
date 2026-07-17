# Brain-RS 架构设计

> 面向编码 Agent 的**项目知识索引与检索引擎**（Rust 实现）。
> 目标：把一个代码仓库编译成"代码结构事实 + 人写知识单元"的可检索索引，
> 让 Agent 在改动前能快速拿到"这块代码是什么、涉及哪些知识、改了会影响谁"。

---

## 0. 红线（不可逾越的设计约束）

这些约束是刻意的，任何改动都不得违反：

1. **默认扫描器不依赖任何编译器/构建系统**
   - 不调用 Clang / clangd / `compile_commands.json`
   - 不调用 Unreal 构建工具、UBT、任何项目脚本
   - 只做**只读、编译器无关、低开销的词法扫描**
   - 代价：结构事实有**边界**（见 §7 已知限制），必须如实标注，不得假装拥有语义级精度。

2. **不读取旧版 `.pi` 配置**
   - 本引擎是干净重写，自带 `brain.toml`，绝不回读历史硬编码配置。

3. **对目标工程零副作用**
   - 只读源码；所有产物写入独立的 `.brain/` 状态目录（已 gitignore）。

---

## 1. 数据流总览

**一项目一颗大脑，共享知识一包一库；项目根只多一个 `.brain/`。** 引擎二进制共用；
每个项目把配置、私有知识、项目级 pack、索引产物全部收敛在 `<项目>/.brain/`
（`brain.toml` + `knowledge/` + `packs/` + `index/`，仅 index 进 gitignore）。
可复用的生态知识以 **pack**（共享知识库）形式存在——`<引擎>/packs/<名>/` 或
`<项目>/.brain/packs/<名>/`（项目优先），每个 pack 有自己独立的索引库，
**一个知识库 = 一个数据库**，库间永不串扰。

```
 项目侧                                  引擎侧（共享）
 ┌─ <project>/brain.toml ─────────────┐
 │  scan (代码层)    compile (私有知识) │   compile --pack (共享知识)
 │   ▼                  ▼              │    ▼
 │  symbols/edges   nodes/claims/refs  │   nodes/claims/refs (未解析)
 │   └──────┬───────────┘              │    │
 │          ▼                          ▼    ▼
 │   <project>/.brain/index/brain.db    packs/<名>/.brain/pack.db
 └──────────┬───────────────────────────┘
            ▼        query：多脑扇出 + 全局 RRF 融合
     项目脑 + 各启用 pack 脑 → 按 brain 标注来源
```

- **`scan`**：并行、增量地把源码扫成 `symbols` / `edges` / `files`（仅项目脑有代码层）。
- **`compile`**：把项目 Markdown 知识切成 Knowledge Unit，解析 claims / 证据 / 符号交叉引用。
- **`compile --pack`**：把 pack 文档编成独立 pack 库；无代码层，符号绑定全部**延迟**到查询时。
- **延迟绑定（late binding）**：pack 里的 evidence/mention/claim 验证在 query 时对照
  **当前查询项目**的代码索引解析——共享知识代码无关，绑到具体项目才谈得上"对不对"。
- 两步分离：代码结构变化频繁（scan 增量快），人写知识变化较少（compile 全量重建即可）。

---

## 2. 数据库 Schema

单一 SQLite 文件 `.brain/index/brain.db`。主库 PRAGMA：`WAL` + `synchronous=NORMAL` + `temp_store=MEMORY`。

| 表 | 用途 | 关键点 |
|----|------|--------|
| `files` | 增量核心：每个文件的 `hash` / `mtime` / `size` | mtime+size 快速判未变；hash 兜底 |
| `symbols` | 代码符号（class/struct/function/…） | 自增 PK；索引 name / qualified_name / file |
| `edges` | 依赖边 | 文件级 import/include + **符号级 call（函数调用）** |
| `nodes` | Knowledge Unit（文档切段） | `parent_id` 树、`heading_path` 上下文信封、`status` 门禁 |
| `nodes_fts` | FTS5 全文索引（external content） | 触发器与 `nodes` 同步；BM25 排序 |
| `claims` | 论断 / 边界（Key Claims、Boundaries 的 bullet） | `kind` = claim / boundary |
| `node_refs` | 文档 ↔ 代码符号交叉引用 | `ref_kind` = evidence / mention；claimed vs resolved |
| `metadata` | 扫描/编译时间戳、扫描器模式 | |

---

## 3. 扫描流水线（`scan`）

**并行提取 + 分片并行写 + 串行归并**，见 `scanner.rs`：

1. **串行 walk**：`collect_candidates` 遍历配置目录，按扩展名/排除规则/大小过滤，
   顺手记录每个文件的 `mtime` / `size`。
2. **预载指纹**：`load_known_files` 一次查询把 `path → (hash, mtime, size)` 读进内存。
3. **分片并行**：候选文件轮询分成 `min(线程数, 8)` 份，每个 rayon worker：
   - **快路径**：`mtime + size` 与旧指纹一致 → 判未变，**完全不读文件**（增量的核心加速）。
   - **慢路径**：读文件 → blake3 哈希；哈希与旧值相同（如 touch）→ 仍判未变。
   - **变更**：跑正则抽取 symbols/edges，写入 worker **私有的 `shard_k.db`**。
4. **串行归并**：`merge_shards` 逐个 `ATTACH` 分片 → 删旧行 → `INSERT ... SELECT` 灌回主库 → `DETACH`。
   - 每次只挂一个分片（绕过 SQLite ATTACH 上限 10）。
   - `INSERT` 显式列名、不含 `id`，由主库重新自增，避免分片间 id 冲突。
5. **收尾**：prune 消失的文件，更新 metadata，删除分片目录。

**为什么这样能安全并行写**：SQLite 单写者锁是"每个 db 文件一把"。
worker 各写各的分片文件（各自独立连接、独立锁），永不碰主库；
`rusqlite::Connection` 非 `Send`/`Sync`，编译器强制保证 worker 不会误触主库连接。

> 性能定位：Lyra（725 文件 / 4546 符号）全量约 2.4s、纯增量约 1.4s。
> 此规模下写入本非瓶颈；分片并行写是为**知识库规模扩张**做的前置投资。

---

## 4. 知识单元切分（`compile` → `split_into_units`）

不再"整篇文档一个节点"，而是沿 **ATX 标题层级**切成自包含的 Knowledge Unit：

- **文档根**始终存在（复用开头的 `# H1`，否则用文件名合成），承接前言与孤儿段落。
- 用标题栈维护祖先链：`###` 自动归属于最近的 `##`。
- 每个 Unit 携带：
  - **Context Envelope**：`heading_path`，如 `Weapons > Weapons Module > Data Flow`。
  - **parent_id**：构成文档内的树。
  - **围栏代码块保护**：``` ``` / `~~~` 之间的 `#` 不会被误判为标题。

### Chunk Contract（进索引前的门禁 · 可审计）

`evaluate_contract` 是每个 Unit 进入检索索引前必须通过的门禁。它由**命名规则**组成，规则失败时产出带原因的 `ContractViolation`（持久化到 `contract_violations` 表），而非只给一个不透明的 status：

- `empty-leaf`（severity=quarantine）：空标题、无正文、无子节点 → 隔离，**不进检索**。
- `thin-content`（severity=degrade）：正文实质字符 < 30 → 降级，进检索但可降权。
- `missing-envelope`（severity=degrade）：无 `heading_path` 上下文信封 → 降级。
- 结构性标题（正文空但有子节点）为组织用途，直接放行 `accepted`。

最终 status 取最严重的一条违规（有 quarantine → quarantined，否则有 degrade → degraded，否则 accepted）。检索（`query`）只返回 `accepted` / `degraded`，隔离项被排除。

门禁**可审计**：`brain contract` 汇总通过率并逐条列出被降级/隔离的 Unit、命中的规则、原因与源码位置，判定过程透明可复现。

---

## 5. Claims / Evidence / 交叉引用

在切分的同时，对每个 Unit 做结构化抽取：

- **Claims**（`claims` 表）：标题含 "Claim" 的 section 每条 bullet → `kind=claim`；
  含 "Boundar" 的 → `kind=boundary`。把论断/边界变成一等公民行。
- **可信度分级**（两条正交轴）：
  - `source`：`extracted`（机械可验证事实）vs `inferred`（语义判断）。作者可用
    `[extracted]` / `[inferred]` 前缀显式标记；无标记时带 `` `Sym` defined at `路径:行号` ``
    证据绑定的论断自动算 extracted，其余 inferred。
  - `verification`：引擎对位置绑定的核查——`verified`（claimed 文件与代码索引解析一致）/
    `drift`（解析到别处）/ `unresolved`（符号消失）/ `unverifiable`（无绑定）。
    项目脑 compile 时核查；pack 脑查询时延迟核查（见 §1）。Evidence Packet 的
    answerability 把 verified extracted claims 当作最强 grounding 信号，
    drift / 标了 extracted 却不可验证的论断会进 warnings。
- **Evidence**（`node_refs`, `ref_kind=evidence`）：解析 `` `符号` defined at `路径:行号` ``，
  记录文档**声称**的定义位置（`claimed_file/line`），即使代码里解析不到也保留——用于暴露漂移。
- **Mention**（`node_refs`, `ref_kind=mention`）：正文里所有反引号符号，
  去 `symbols` 表解析，**仅保留能解析到**的（去噪），建立"文档段 ↔ 代码定义"链接。

### 漂移检测（doc/code drift）

`refs` 命令对 evidence 优先显示文档权威的 `claimed` 位置；
当 `claimed` 与引擎 `resolved` 不一致时输出 `⚠ drift`。
（历史上 drift 曾暴露一个扫描器缺陷：词法扫描把前向声明 `class ULyraWeaponInstance;`
当成定义、又把 UE 导出宏 `LYRAGAME_API` 误当类名，导致 `resolved` 选错文件。
该缺陷已在扫描器修复——前向声明不再记为定义、导出宏被跳过；符号数从 4511 降到 3190，
`ULyraHealthComponent` 等现在正确解析到真定义。drift 检测保留用于捕获后续的文档/代码漂移。）

---

## 5.5 多路检索融合（`query` · B4）

`query` 不再是单路 BM25，而是**多脑扇出 + 三路召回 + Reciprocal Rank Fusion (RRF)**：
项目脑与每个启用 pack 脑各自独立跑三路召回，命中打上 `brain` 来源标记后全局 RRF 融合；
pack 的 symbol/graph 路借助**项目脑**的代码索引解析符号，再反查 pack 自己的 node_refs。
`locate` / `graph` 只查项目脑（代码层只在项目脑）；`refs` / `contract` / `status` 分脑汇总。

| 路 | 信号 | 权重 | 召回什么 |
|----|------|------|----------|
| **bm25** | FTS5 全文 + BM25 | 1.0 | 自然语言相关性（词法） |
| **symbol** | 查询里的代码符号 → `node_refs` 反查引用它的知识单元 | 2.0 | 精确、高置信（"讲这个符号的段落"） |
| **graph** | 符号的图邻居 → 引用邻居的知识单元 | 0.6 | 关联召回（"你问的东西周围的事") |

- **符号候选**：对查询复用 `mentioned_symbols`（多驼峰/下划线启发式），再用 `symbols` 表校验存在；纯自然语言查询无符号 → 干净退化为纯 BM25（无回归）。
- **graph 两跳桥接**：① 符号级 call 邻居（edges 函数↔函数）；② 文件级——符号定义文件 → include 邻居文件 → 其中的符号。因 `edges.target_file` 存的是 `#include` 原始字面量（部分路径）、而 `symbols.file` 是完整相对路径，两者**按 basename 桥接**。
- **RRF 融合**：`score(node) = Σ_route w / (K + rank)`，K=60。不需归一化不同路的分数量纲，只用排名。
- **provenance 透明**：每个命中标注命中了哪些路 `⟨bm25+symbol+graph⟩`，排序可解释、可审计（`--json` 带 `routes` 字段）。

**融合的价值**（实测 `query ULyraEquipmentManagerComponent`）：Top 命中是 Equipment 三路齐中；而 AbilitySystem 的知识单元由 `⟨graph⟩` **单独召回**——这些文档根本不含查询词，纯 BM25/符号都召不到，靠 include 图的关联被带出来。

---

## 5.6 分层粒度节点（`query --scope` · B5）

同一份文档被切成不同粒度的知识单元。**文档根**的 scope 由 frontmatter 的层级字段声明（关注范围阶梯），
**内部小节**的 scope 按树深度：

| scope | 层 | 来源 |
|-------|-----|------|
| `project` | 架构（整个项目） | doc 根 + frontmatter `architecture:` |
| `domain` | 领域（跨模块功能区） | doc 根 + frontmatter `domain:`（别名 `system:`，如 Combat） |
| `module` | 模块（单代码单元） | doc 根 + frontmatter `module:`（默认） |
| `feature` | 特性（原子事物） | doc 根 + frontmatter `feature:`（+`module:` 归属） |
| `section` | 主干章节 | doc 根的直接子节点（树深度 1，通常是 `##`） |
| `subsection` | 细节 | 更深的嵌套节点（树深度 ≥2，`###`+） |

- **根 scope 按 frontmatter 层级字段、内部按树深度**：内部小节无论文档从 `#` 还是 `##` 起头都稳定（`depth = 祖先数`）。
- **检索意图分层**：`query --scope <overview|unit|section|detail|all>` 把不同粒度需求路由到对应 scope：
  - `overview` → `project` + `domain`（大图：架构与领域）
  - `unit` → `module` + `feature`（一个具体单元/事物）
  - `section` → `section`（主干章节）
  - `detail` → `subsection`（深层细节）
  - `all`（默认）→ 不过滤
- **实现**：融合阶段 over-fetch 到 `max_results×4`，在 fetch 时按 scope 过滤再截断到 `max_results`，各路 SQL 不变。

**实测**（同一 `query "weapon damage combat"`）：`--scope overview` 只回领域节点（Combat System）；
`--scope unit` 只回模块/特性节点（Weapons Module）；`--scope detail` 只回 Class Responsibilities 等深层细节——同一查询、多种粒度、各取所需。

---

## 5.7 单轮自足的 Evidence Packet（省 Agent 交互轮次）

**动机**：引擎单次 40–110ms（冷启动进程），可忽略；真正贵的是 Agent 的**交互轮次**——每轮夹一个秒级 LLM 往返。所以优化目标不是"引擎更快"，而是"**一次 `query` 就是一个完整决策单元**"，把多轮压进单轮的信息密度。

针对两处最常见的冗余轮次动刀：

1. **默认就组装证据包**（原来默认只给摘要列表 → 逼出第二轮 `--assemble`）。现在 `query` 默认返回 top-3 完整 Evidence Packet；`--brief` 才回退到轻量列表（快速探索用）。
2. **内联源码片段**（原来 answerability 不足时 `fallback_to_source` → Agent 再发一轮去读 `file:line`）。现在组装时直接把每条证据 `resolved_file:line` 附近的源码窗口（前 1 + 后 5 行，带行号）**读进包里**。即使文档知识不充分，Agent 在同一轮就同时拿到「文档怎么说 + 源码实际是什么」，无需再读文件。
   - 预算上限 6 个符号/包，primary 优先、supporting 补位；per-file 行缓存避免重复读。
   - 读不到（文件删/移）→ 空 excerpt，本身即 drift 信号，不报错。

**代价**：默认组装 + 读 6 个源码文件，单次 45ms → **58ms**（仅 +17ms），仍远低于一次 LLM 往返。

**效果**：Agent 一次有效知识获取从「典型 2 轮、需验证 3–4 轮」压到**理想 1 轮**——拿到即含「答案 + 自评估(answerability) + 分层证据 + 内联源码 + 行动建议(recommended_action)」，`sufficient` 直接用、`partial` 也能就地核对内联源码，不必再发轮次。

---

## 6. CLI 命令

| 命令 | 作用 |
|------|------|
| `init` | 生成知识根模板（项目：`.brain/brain.toml` + `.brain/knowledge/`；`--pack <目录>`：包根）；项目与包共用同一模板源，幂等不覆盖 |
| `scan` | 并行增量扫描源码 → symbols / edges / files |
| `compile` | 编译项目知识文档 → Knowledge Units / claims / node_refs；`--pack <目录>` 编译共享知识包到 `<pack>/.brain/pack.db` |
| `query <text>` | **三路融合检索**（BM25 + 符号 + 图，RRF）；**默认组装 top-3 自足 Evidence Packet（含内联源码）**；`--brief` 出轻量列表；`--scope <overview\|unit\|section\|detail\|all>` 选粒度 |
| `locate <symbol>` | 定位代码符号定义处 |
| `refs <symbol>` | **反查**：哪些知识单元引用该符号（含 evidence/mention/drift） |
| `graph <kind> <symbol>` | 图查询：callers/callees（符号级调用，可多跳）、deps/dependents（文件级依赖）、impact |
| `status` | 索引统计（各表计数、门禁分级、时间戳） |
| `contract` | **Chunk Contract 审计**：按命名规则列出被降级/隔离的知识单元及原因、位置 |

全局参数：`--project-root`、`--config`、`--state-dir`；多数命令支持 `--json`。

---

## 7. 已知限制（如实标注，非缺陷隐藏）

1. **C++ 类/结构体的前向声明与导出宏已修复；函数原型仍是近似**：
   `class Foo;` 前向声明不再记为定义，UE 导出宏（`LYRAGAME_API` 等）在类名前被跳过，
   `class`/`struct` 现在正确解析到真定义（符号数 4511→3190，噪音清除）。
   但**函数原型** `void Foo();` 与定义仍可能同名并存，`locate` 对函数可能仍选到声明；
   Evidence 的 claimed 位置更可信，drift 会提示。
2. **call 边是词法近似**：函数作用域用花括号深度跟踪，可能被字符串/块注释/宏/lambda
   里的花括号干扰；被调符号只记名字、不解析到定义文件（同名方法无法区分具体类）。
   常见 UE 宏（TEXT/LOCTEXT/UE_LOG/check/ensure…）已过滤，仍有少量噪音。
   `graph callers/callees` 已可用；Python 用缩进作用域，暂不产出 call 边。
3. **edges 按名字串存储，非符号 id 外键**：跨文件关联是词法近似，非语义精确。
4. **检索为词法/结构多路融合，无向量语义检索**：`query` 已是 BM25 + 符号 + 图三路 RRF 融合，但仍无向量/嵌入语义召回（B8 规划中）；graph 路的产出受 edge 质量约束（见 §2）。
5. **无 MCP / Agent 集成**：目前是 CLI 引擎。

这些边界都是 §0 红线（不用编译器）的直接代价，是**已知且可接受**的取舍。

---

## 8. 目录结构

```
brain-rust/
├─ brain.toml            # 配置：scan / index / retrieval
├─ src/
│  ├─ main.rs            # 命令分发
│  ├─ cli.rs             # 参数与子命令定义
│  ├─ config.rs          # 配置加载与规范化
│  ├─ model.rs           # Symbol / Edge / 检索结果结构
│  ├─ scanner/           # 扫描流水线（mod）+ 按语言分模块
│  │  ├─ mod.rs          #   并行增量扫描 + 分片并行写 + LanguageScanner trait
│  │  ├─ common.rs       #   共享：符号构造、call 噪音过滤、花括号作用域状态机
│  │  ├─ cpp.rs          #   C++：class/struct/func + include + call
│  │  ├─ typescript.rs   #   TS/JS：func/class/import + call
│  │  └─ python.rs       #   Python：def/class + import
│  ├─ storage.rs         # 数据库 schema（主库 + 分片库）
│  ├─ index.rs           # 知识编译、切分、claims/refs、检索
│  └─ graph.rs           # 图查询（call 符号级 / import 文件级）
└─ .brain/index/brain.db # 产物（gitignore）
```
