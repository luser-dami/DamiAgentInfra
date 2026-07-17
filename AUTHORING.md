# 知识文档维护规范（Authoring Guide）

> 本文件是**给人读的规范**，放在引擎仓库根目录，**不会被引擎索引**
> （引擎只扫 `brain.toml` 里 `docs_dirs` 配置的项目知识根，以及 `enabled_packs` 启用的共享包）。
>
> 知识有两种归属：**项目私有知识**放 `<项目>/knowledge/`（编进项目脑）；
> **可复用的生态知识**放 `packs/<包名>/`（文档直接在包根下，一包一库，
> 用 `brain-rs compile --pack packs/<包名>` 构建，项目用 `enabled_packs` 启用）。
> 写作规则两者完全通用。

知识文档是整个引擎的**燃料，也是唯一权威来源**：代码里抽的是"机械事实"（符号/调用/依赖），
而"为什么这么设计、职责边界、端到端流程"只能来自这些手写文档。文档写得好不好，
直接决定 Agent 检索到的答案有没有用。

本规范回答三件事：**放哪里**（组织）、**怎么写**（新建）、**怎么改**（维护）。
所有规则都从引擎实际解析行为反推，标注了对应的代码依据，不是拍脑袋的"最佳实践"。

---

## 0. 引擎解析契约（硬约束速查）

写文档前必须知道引擎**实际会怎么解析**你的 Markdown。下面每条都是硬约束。

| 元素 | 引擎行为 | 代码依据 |
|------|----------|----------|
| **Frontmatter** | 第一行非空必须是 `---`，到下一个 `---` 结束；只读 `key: value` 简单行 | `chunk.rs::detect_frontmatter` / `parse_frontmatter` |
| 层级字段 | `architecture:`→根 scope `project`；`domain:`（别名 `system:`）→`domain`；`feature:`→`feature`；`module:`→`module`（默认） | `compile_documents` |
| `module:` 值 | 取 `/` 后最后一段（`LyraGame/Weapons` → `Weapons`）作 Context Envelope 的 module 身份 | `compile_documents` |
| 其他 key（tags/source/feature-slug） | **被忽略但不报错**，纯给人读 | `parse_frontmatter` |
| **ATX 标题** | `#`~`######`，`#` 后**必须有空格**（`#tag` 不算标题） | `chunk.rs::parse_heading` |
| 标题层级 | 建树：`###` 归属最近的 `##`；文档根 scope 由 frontmatter 层级字段定，`##`=section，`###`+=subsection | `split_into_units` |
| 围栏代码块 | ` ``` ` / `~~~` 之间的 `#` 不会被误判为标题 | `split_into_units` |
| **章节 kind** | 由**标题关键词**决定语义类型（见 §4） | `extract.rs::classify_kind` |
| **Claims** | 标题含 `claim` 或 `boundar` 的 section，每个 `- `/`* ` bullet 成为一条 claim | `classify_claim_section` |
| **Claim 可信度标记** | bullet 前缀 `[extracted]`=机械可验证事实 / `[inferred]`=语义判断（不区分大小写，存储时剥离）；无标记时带 `defined at` 绑定的自动算 extracted | `extract.rs::parse_claim_marker` + `compile_documents` |
| **Evidence** | 标题恰为 `Evidence` 的 section，每个 bullet 解析 `` `符号` ... `路径:行号` `` 为 primary 证据 | `compile_documents` |
| **符号 mention** | 其余 section 里的反引号符号 + 明文 CamelCase/snake_case，**仅保留能在代码里解析到的** | `extract.rs::mentioned_symbols` |
| **门禁** | 空章节→隔离(不进检索)；正文<30 实质字符→降级；无信封→降级 | `contract.rs::evaluate_contract` |

**一句话**：标题关键词决定语义、bullet 决定论断/证据、反引号+驼峰决定代码锚点、正文长度决定能否进检索。

---

## 1. 放哪里：按“关注范围”分四层

引擎用 `scope` 区分粒度（`query --scope`）。文档组织用**一把尺子**——**关注范围（scope of concern）**：
这份知识管多大一摊？从大到小四层，加一个内联层。

| 层 | 名称 | 关注范围 | 跨度 | frontmatter | 目录 |
|----|------|----------|------|-------------|------|
| L0 | **架构 Architecture** | 整个项目 | 全部 | `architecture:` | `docs/` 根 |
| L1 | **领域 Domain** | 一个功能领域 | 多个代码单元 | `domain:` | `docs/domains/` |
| L2 | **模块 Module** | 一个代码单元 | 一个文件夹 | `module:` | `docs/modules/` |
| L3 | **特性 Feature** | 一个原子事物 | 单一拥有者 | `feature:`(+`module:`) | `docs/features/` |
| — | 细节 Detail | 文档内部 | — | 内联 `###` | （随宿主文档） |

**关键前提**：引擎的 `scope` 按**树深度**决定内部小节（`##`=section、`###`+=subsection），
而**文档根**的 scope 由 frontmatter 声明的层级字段决定（`chunk.rs` + `compile_documents`）。
所以“放哪一层”＝“frontmatter 写哪个字段”，直接可控。

### 1.0 架构文档（`architecture:`）
- **写什么**：整个项目的入口视图——技术栈、顶层目录布局、核心约定、模块地图。回答“这个代码库是什么、怎么组织的”。
- **典型**：`Architecture.md`（一个项目一份，Agent 的第一站）。
- **frontmatter**：`architecture: <ProjectName>`。
- **放置**：`docs/` 根目录。
- **可选**：小项目可以省略；大项目强烈建议有，它是 Agent 建立全局心智的起点。

### 1.1 领域文档（`domain:`）
- **写什么**：一个**功能领域**的跨模块视图——通常是一条端到端流程。回答“<某领域> 在整个代码库里怎么运转”。
- **典型**：`Combat.md`（武器开火→伤害→血量，跨 Weapons/AbilitySystem/Character）；`Networking.md`。
- **特征**：自己不定义类，只引用各模块的类，把它们串成一条流程线（**无单一代码拥有者**）。
- **frontmatter**：`domain: <DomainName>`，**不写** `module:`。
- **放置**：`docs/domains/`。
- **为什么叫“领域”不叫“系统”**：“系统”在游戏开发里被 ability system / input system 等运行时概念占用，歧义大；“领域”专指功能划分，无碰撞。

### 1.2 模块文档（`module:`）
- **写什么**：一个**代码单元**的职责、架构、数据流、边界。回答“<这个文件夹> 负责什么、怎么组织”。
- **典型**：`Weapons.md` / `Character.md`（一个 `Source/LyraGame/<X>/` 目录一份）。
- **frontmatter**：`module: LyraGame/<ModuleName>`。
- **放置**：`docs/modules/`。
- **注意**：这里的“模块”指**一个内聚的代码单元/文件夹**，**不是** UE 的 Build module（`.Build.cs`）。Lyra 的 Weapons 是 `LyraGame` 这个 UE module 里的子文件夹。

### 1.3 特性文档（`feature:` + `module:`，**独立成文件**）
- **写什么**：一个**原子的、具体的、关键的事物**——一个技能/ability、一个得分算法、一个协议、一个复杂状态机。
  内容详尽、独立演进、值得被单独检索。**它是最小的独立单位**（所以叫“特性”而非“专题”——专题隐含宽泛主题，与其原子本质相反）。
- **典型**：`HeroDash.md`（冲刺技能的 Task 编排）、`EliminationScoring.md`（淘汰得分算法）。
- **frontmatter**：`feature: <feature-slug>` + `module: LyraGame/<拥有它的模块>`（`module:` 声明归属，建立 Context Envelope 身份）。
- **放置**：`docs/features/`。
- **为什么独立成文件而非塞进模块文档的 `###`**：内容量大会撑爆模块文档；它独立于模块其余部分演进；它本身值得完整的 Context Envelope 和自己的 Evidence 与检索入口。
- **实测**：查“淘汰得分 streak 加成”，Top-3 全部命中该特性文档——独立成文件不丢检索，反而给了这块关键知识专属入口。

### 1.4 内联细节（`###` 小节，**不单独成文件**）
- **写什么**：**依附于宿主文档、不值得独立检索**的细节——类职责表、结构体清单、一小段说明。
- **归属**：作为模块/特性文档里的 `###` 三级小节（如 `### Class Responsibilities`）。
- **原因**：这类细节脱离上下文就没有意义；引擎按树深度自动标为 `subsection`，`--scope detail` 能命中。

### 1.3 vs 1.4 怎么选（关键判断）

> **问一句话：“有人会**单独**来查这块知识吗？”**
> - 会（得分算法、冲刺技能、核心公式）→ **§1.3 独立特性文件**。
> - 不会，它只是宿主的一个组成部分（类职责表）→ **§1.4 内联 `###`**。
>
> 判断依据是“**是否值得独立检索**”，**不是内容长短**——3 行的核心公式可能值得独立，30 行的类职责表可能只配当 `###`。

### 1.5 跨模块的编排型特性（复杂 ability / 多 Task 技能）

**场景**：一个复杂 ability 编排多个 Task（位移/动画/摄像机 Task 各属不同模块），读者要看“Task 之间怎么跳”。
这类知识**跨多个模块**，容易纠结“算哪一层”。

**先破误区**：“跨几个模块”**不是**决定归属的轴——几乎每个 ability 都碰动画/位移/摄像机。真正的判断轴是：
1. **有没有单一“拥有者”？** 技能有一个 GA 类定义它；它调的各 Task 是**协作者/依赖**，不是身份。
2. **读者在查什么？** 查“这技能怎么编排 Task”，价值在**编排流程**（Task A→B→C 怎么跳、各自等什么、中断跳哪）。

**结论：它是一篇 §1.3 特性文档**（编排型），身份归属 = 定义该 ability 的模块：
- frontmatter：`feature: <ability-slug>` + `module: LyraGame/<定义该 GA 的模块>`；
- 主体是一个 **Task Orchestration Flow**（`## Data Flow`）：图里用**各 Task 的真实类名**（跨模块也没关系，引擎会解析成代码锚点）；
- **Edge Cases** 写清中断/分支时控制权往哪跳——这是这类文档的灵魂；
- Evidence 引用各协作模块的核心符号，建立**跨模块锚点**。

**关键机制（为什么不用把所有模块塞进一篇）**：检索时 B4 的 **graph 路**会顺着 ability 的符号，
**自动把各协作模块的 Task 知识缝进结果**。你只写这一篇编排文档 + 引用真实类名，
查询时“位移 Task 怎么工作”“摄像机怎么切”由各自模块的文档补上。

> **实测**：一篇跨 AbilitySystem/Character/Camera 的 ability 编排文档，用符号查询后，
> 结果 Top 同时包含该编排文档（三路齐中 `⟨bm25+symbol+graph⟩`）**和** Character 模块的 Task 节点——
> 一篇编排文档 + graph 路自动拼出跨模块全貌，**不需要**巨无霸文档。

**什么时候反而用 `domain:`？** 当你写的**不是某一个具体技能**，而是“**所有 dash 类技能的通用模式**”这种
没有单一拥有者的横切视图时——那是领域级的模式，不是一个 feature。

### 归属决策速查

| 你要写的东西 | 层 | frontmatter |
|--------------|----|-------------|
| 整个项目的入口/布局/约定 | 架构 | `architecture:` |
| 跨模块的一条流程（无单一拥有者） | 领域 | `domain:` |
| 一个模块的整体职责/架构 | 模块 | `module:` |
| 一个值得单独查的原子事物（算法/技能/协议） | 特性 | `feature:` + `module:`(归属) |
| **复杂 ability 的 Task 编排（跨模块但有单一 GA 拥有者）** | **特性** | **`feature:` + `module:`(GA 所在模块)** |
| 宿主文档的一个不值得单独查的组成部分（类职责表） | 内联 `###` | —（随宿主文档） |

### 检索粒度对应（`query --scope`）

`scope` 层级与检索粒度过滤一一对应：

| `--scope` | 命中的层 | 意图 |
|-----------|----------|------|
| `overview` | project + domain | “给我大图”——架构与领域 |
| `unit` | module + feature | “给我某个具体单元/事物” |
| `section` | 文档内 `##` 章节 | 主干章节 |
| `detail` | 文档内 `###` 小节 | 深层细节 |
| `all`（默认） | 不过滤 | 全部 |

### 推荐目录结构（项目知识根 / pack 根通用，文档直接在根下）
```
knowledge/              ← 项目私有知识根（docs_dirs，默认 ["knowledge"]）
  Architecture.md       ← L0 架构（architecture:）项目入口
  domains/              ← L1 领域（domain:）跨模块流程
    Combat.md
    Networking.md
  modules/              ← L2 模块（module:）单代码单元
    Weapons.md
    Character.md
  features/             ← L3 特性（feature: + module:）原子事物
    HeroDash.md
    EliminationScoring.md
```
> 引擎用 `walkdir` **递归**扫描知识根，子目录零配置生效；隐藏目录（如 `.brain`）自动跳过。
> `system:` 仍作为 `domain:` 的**向后兼容别名**被解析，旧文档不会失效，但新文档一律用 `domain:`。
> 共享 pack 同理：`packs/<包名>/` 根下直接放 `Architecture.md` / `domains/` / `modules/` / `features/`。


## 2. 标准文档骨架（模块级模板）

新建模块文档，**从这个骨架开始**。每个 `##` 标题都精心选词以触发正确的 kind。

````markdown
---
module: LyraGame/<ModuleName>
tags: [<给人读的关键词，引擎忽略>]
source: manual
---

# <ModuleName> Module

<一句话概述：这个模块提供什么。这段会成为文档根的 summary。>

## Context

- **Module path:** `Source/LyraGame/<ModuleName>/`
- **Dependencies:** <依赖的其他模块>
- **Consumers:** <谁依赖本模块>

## Architecture

```
<类继承/组合关系图，用等宽图示>
```

### Class Responsibilities

| Class | Parent | Role |
|-------|--------|------|
| `UFooClass` | `UBarBase` | <一句话职责> |

### Key Structs

| Struct | Usage |
|--------|-------|
| `FFooData` | <用途> |

## Data Flow

```
<端到端流程图，节点用真实类名/函数名（CamelCase）>
```

## Key Claims

- [extracted] `USymbol` is defined at `Source/LyraGame/<ModuleName>/<File>.h:<line>` and <机械可验证的事实>。
- [inferred] <基于多处代码的语义判断，独立成 bullet、能被单独引用>。

## Boundaries

- This module does **not** <明确不负责什么>。
- <边界/限制，帮助 Agent 判断"这里找不到就别找了">。

## Evidence

- `USymbol` defined at `Source/LyraGame/<ModuleName>/<File>.h:<line>`
- <每个核心符号一条，格式严格：`符号` + `路径:行号`>
````

领域级文档（`domain:`）同构，差异：
- frontmatter 用 `domain:`（不写 `module:`）；
- 概述强调"跨哪些模块"；
- **Data Flow 是核心**（领域文档的存在理由就是这条流程）；
- Evidence 引用的是**其他模块**的符号（跨模块锚点）。

### 特性文档模板（§1.3）

特性文档比模块文档**更自由**：核心是把一个原子事物讲透，章节按内容组织，不必套用全部标准节。
但**身份、边界、证据三节不能省**。

````markdown
---
feature: <feature-slug，如 elimination-scoring>
module: LyraGame/<OwningModule>
tags: [<给人读的关键词>]
source: manual
---

# <Feature Name>

<一句话：这是什么、为什么独立成文件（关键且独立演进）。成为文档根 summary。>

## Context

- **Owning module:** `Source/LyraGame/<OwningModule>/`
- **Trigger / Inputs:** <什么触发它 / 输入是什么>
- **Consumers:** <谁用它的产出>

## <主题主体，如 Algorithm / Protocol / State Machine / Task Orchestration Flow>

<把算法/协议/状态机/编排流程讲透。公式用代码块；深层分支用 ### 展开（自然成 subsection）。>

## Edge Cases

- <边界条件、特殊分支——这类知识的真正价值往往在这里>。

## Boundaries

- This <feature> does **not** cover <明确不管什么>。

## Evidence

- `USymbol` defined at `Source/LyraGame/<OwningModule>/<File>.h:<line>`
````

要点：
- **`feature:` 决定根 scope 为 `feature`**（`query --scope unit` 可命中），**`module:` 声明归属**（建立 Context Envelope 身份）。两者都要写。
- 主体章节标题若不在 §4 关键词表内，会落成通用 `section`——**可接受**（特性主体本就是自定义内容），但若想精排，可给主体小节起个含关键词的名（如把编排流程叫 `## Data Flow`）。
- **Edge Cases / Boundaries 是特性文档的灵魂**：算法/技能的价值一半在边界条件，务必写全。

---

## 3. 每个标准章节怎么写（对应引擎抽取）

| 章节标题 | 触发 kind | 引擎从这里抽什么 | 写法要点 |
|----------|-----------|------------------|----------|
| `## Context` | context | 符号 mention | 列路径/依赖/消费者，用反引号包路径 |
| `## Architecture` | architecture | 符号 mention | 类关系图；类名用反引号或裸 CamelCase 均可被抽 |
| `### Class Responsibilities` | responsibility | 符号 mention | 表格，`Class` 列用反引号 → 建立代码锚点 |
| `### Key Structs` | data_structure | 符号 mention | 同上 |
| `## Data Flow` | data_flow | 符号 mention | 流程图里的类名/函数名会被抽为锚点（**驼峰即可，无需反引号**） |
| `## Key Claims` | design_decision | **claims**（每 bullet 一条） | 每条论断自包含、可单独引用；用 `[extracted]` / `[inferred]` 前缀标可信度（§0）；含 `符号` 会关联代码 |
| `## Boundaries` | boundary | **boundary claims** | 用"does **not**"句式明确边界 |
| `## Evidence` | evidence | **primary 证据绑定** | 严格格式 `` `符号` defined at `路径:行号` `` |

**关键**：
- **Data Flow 的图示里，裸写 `ULyraHealthComponent` 就会被抽成代码锚点**（明文 CamelCase 抽取），不必强制加反引号——但加了更稳。
- **Evidence 的 `路径:行号` 会被引擎去代码里核对**：对得上 → 证据可信；对不上 → 触发 `⚠ drift` 提示（说明代码改了、文档过时）。这是文档保鲜的抓手。

---

## 4. kind 关键词对照（标题选词表）

`classify_kind` 靠标题**关键词子串**匹配（大小写不敏感）。想要某个 kind，标题必须含对应词：

| 想要 kind | 标题需含 | 例 |
|-----------|----------|-----|
| data_flow | `flow` / `data flow` | `## Data Flow` |
| architecture | `architect` | `## Architecture` |
| responsibility | `responsib` | `### Class Responsibilities` |
| data_structure | `struct` | `### Key Structs` |
| design_decision | `claim` | `## Key Claims` |
| boundary | `boundar` | `## Boundaries` |
| dependency | `depend` | `## Dependencies` |
| evidence | `evidence` | `## Evidence` |
| context | `context` | `## Context` |
| impact | `risk` / `impact` | `## Impact & Risks` |
| （其他） | — | 落为通用 `section` |

> 反过来：标题**随便起名**会落成无语义的 `section`，检索时无法按 kind 精排。**遵循标准标题名**。

---

## 5. 怎么改：维护流程

### 5.1 改完必须重新编译
```
brain-rs --project-root <项目根> compile          # 项目知识 → 项目脑
brain-rs compile --pack packs/<包名>              # 共享包知识 → 包自己的库
```
- 文档编译是**全量重建**（`DELETE FROM nodes` 后重切），不是增量——但 71 个节点秒级完成，无需担心。
- 代码扫描（`scan`）是增量的；文档改动**只需 `compile`**，不必重新 `scan`。

### 5.2 三个自查命令（文档质量反馈闭环）
改完文档，用引擎**自查**，而不是凭感觉：

1. **看门禁** —— 有没有写出会被隔离/降级的垃圾章节：
   ```
   brain-rs contract
   ```
   输出会逐条列出 `empty-leaf`（空章节）/`thin-content`（太短）/`missing-envelope`，带原因和行号。目标：新写的章节不出现在里面。

2. **看漂移** —— Evidence 里的 `路径:行号` 是否还对得上代码：
   ```
   brain-rs refs <你引用的符号>
   ```
   出现 `⚠ drift: code index resolved <别的文件>` → 代码位置变了，去更新 Evidence。

3. **看可回答性** —— 你的文档能不能真的回答目标问题：
   ```
   brain-rs query "<你希望这篇文档能回答的问题>" --assemble
   ```
   看命中节点的 `answerability`：`sufficient` 才算合格；`partial/insufficient` 说明证据不足或内容太虚，需要补 claims/Evidence。

### 5.3 改动检查清单
- [ ] frontmatter 首行是 `---`，层级字段正确：`architecture:`/`domain:`/`module:` 之一；特性文档用 `feature:` + `module:`
- [ ] 标准章节（Context/Architecture/Claims/Boundaries/Evidence）用了 §4 的关键词；专题主体自定义标题可例外
- [ ] 新增章节正文 ≥ 30 个实质字符（否则被降级）
- [ ] Key Claims / Boundaries 每条论断独立成 bullet
- [ ] Evidence 格式严格：`` `符号` defined at `路径:行号` ``
- [ ] 跑 `compile` + `contract` 无新增违规
- [ ] 跑 `refs` 无未预期 drift
- [ ] 跑 `query --assemble` 目标问题达到 `sufficient`

---

## 6. 反模式（不要这样写）

| 反模式 | 后果 | 正确做法 |
|--------|------|----------|
| 标题前留缩进 `  ## X` 或用 Setext（`===`） | 不被识别为标题，整段并入上一节 | 顶格 ATX `## X` |
| `#标题`（`#` 后无空格） | 不算标题 | `# 标题`（加空格） |
| 一句话章节 / 空章节占位 | 被 `thin-content` 降级 / `empty-leaf` 隔离 | 要么写够 30 字，要么删掉 |
| 把**不值得单独查**的碎片（一小段说明）拆成独立文件 | 丢失 Context Envelope，检索碎片化 | 作为模块文档的 `###` 小节（§1.4） |
| 把**值得单独查**的深主题（算法）硬塞进模块文档 | 撑爆模块文档、失焦，且随主题改动被迫重编 | 独立成专题文档（§1.3） |
| Evidence 写成散文 `定义在 XX 文件里` | 解析不出符号/行号，不成锚点 | 严格 `` `符号` defined at `路径:行号` `` |
| 论断写成一大段 | 无法被单独抽成 claim | 拆成一条条 `- ` bullet |
| frontmatter 用复杂 YAML（嵌套/数组值当结构） | 只读简单 `key: value`，复杂结构被忽略 | 保持 `key: value` 单行 |
| 非知识文件（README/规范）放进 `docs/` | 被当知识节点索引，污染检索 | 放 `docs/` 之外 |

---

## 7. 已知局限（诚实标注）

- **文档编译无增量**：改一个文档会全量重建所有节点（当前规模秒级，可接受）。
- **无 lint 预检**：目前靠 `compile` 后跑 `contract`/`refs` 事后发现问题，没有"写的时候就报错"的编辑期校验（未来可做 pre-commit 钩子）。
- **kind 靠关键词子串**：标题选词不当会静默落成通用 `section`，无报错——严格遵循 §4 是唯一保障。
- **`docs/` 无 ignore 机制**：任何 `.md` 都会被索引，靠"放对目录"规避，而非配置排除。
