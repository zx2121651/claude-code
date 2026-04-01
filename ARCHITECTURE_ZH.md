# Claude Code 深度源码与架构分析报告

本报告对 Anthropic 发布的终端 AI 编程助手工具 **Claude Code** 的核心源码与运行架构进行深入分析。该项目利用大语言模型（特别是 Claude 3.5/3.7 系列）与终端本地环境无缝结合，并配备了多代理协同（Agent Swarm）以及极其复杂的上下文管理策略。

---

## 1. 系统核心生命周期 (`main.tsx` & `QueryEngine.ts`)

### 1.1 启动与初始化 (`main.tsx`)
系统使用 `Commander.js` 处理命令行参数，在启动之初即开启了高密度的并行预加载（Parallel Prefetching）：
- 并发读取 MDM (移动设备管理) 策略。
- 从 macOS Keychain 中并行读取 OAuth 凭证和旧版 API Key。
- 并发连接 Anthropic 官方 API 并预热 (Warm-up) 指令缓存。
系统通过动态加载（Lazy Loading / dynamic `import()`）延后加载庞大的模块（如 `OpenTelemetry` 和 `gRPC` 追踪模块），以最大限度地降低首次渲染终端 UI（基于 `React` + `Ink`）的延迟。

### 1.2 会话查询引擎 (`QueryEngine.ts`)
`QueryEngine` 类是管理与 LLM 对话状态的“心脏”：
- **流式请求处理**: 引擎异步接收 API 返回的流（Stream），逐个处理 `message_start`, `content_block_delta` 和 `message_stop` 事件。
- **工具挂载与协调**: 遇到 `tool_use` 块时，停止文本渲染并激活对应工具（如执行命令、读文件），然后将结果封装成 `tool_result` 返回给大模型形成闭环。
- **断点恢复机制**: 若在请求过程中遇到模型返回的 API Error (如 `max_output_tokens` 被截断，或 `413 Request Too Long`)，引擎会根据策略自动尝试拦截报错、触发内存折叠 (Context Collapse) 或反应性压缩 (Reactive Compact)，并对断点进行重试。

---

## 2. 上下文与 Token 管理策略 (`src/services/compact/`)

因为大模型有着 200K Tokens 的硬性物理限制，而在执行 `grep` 搜索或读取多个源文件时，Token 会爆炸式增长，因此 Claude Code 采用了非常细分的多级压缩策略，拦截点分布在 `QueryEngine.ts` 每一轮问答的钩子中：

### 2.1 微型压缩 (MicroCompact) & 缓存编辑 (Cached Microcompact)
- 在每轮提问之前触发。它会检查此前所有工具的输出（如搜索出的冗长 `ripgrep` 结果或大型文件内容）。
- 如果内容过长且属于早期的上下文，系统会“摘除”或总结这些工具输出内容。
- **Cached MC (利用 Prompt Caching)**: 利用 API 的 `cache_edits` 指令，动态删除远端缓存树中无用的内容区块，而不是重新发送所有上下文，以此节省高昂的 Cache Creation Tokens 成本。

### 2.2 上下文折叠 (Context Collapse)
- 主要处理**搜索与读取命令**（例如 `cat`, `grep`, `ls` 等只读操作）。
- 它会将连续多次的试错探索折叠为一条带有“系统总结”的摘要（如：“模型读取了3个文件，发现目标函数在 `utils.ts` 中”），彻底抛弃原文以释放高达 90% 的冗余探索过程。

### 2.3 自动压缩 (AutoCompact / Session Memory)
- 当会话 Token 总量逼近阈值（如距离上下文上限还剩 13,000 tokens 时，`isAboveAutoCompactThreshold` 返回 true）。
- 系统挂起当前用户的请求，并在后台单开一条线路请求 LLM：“总结截止到目前的核心任务状态和获取到的知识”。
- 随后替换掉前面所有长篇大论，仅保留系统指令和这条压缩摘要。

### 2.4 反应性压缩 (ReactiveCompact)
- 作为保底机制 (Circuit Breaker)。如果 API 明确返回了 `413 Payload Too Large`，系统不会向用户抛出异常，而是拦截这个错误，强行触发压缩，然后带着精简后的上下文重试用户的问题。

---

## 3. 多代理协作机制 (Agent Swarm & Coordinator)

系统最惊艳的设计之一是其支持原生的 Agent 并发处理。这部分代码主要集中在 `src/coordinator/` 和 `src/tools/AgentTool/`。

### 3.1 协调者模式 (Coordinator Mode)
- **职责**: 协调者充当大脑，它自身**不**直接执行低级任务（如读取文件或执行 Bash），而是通过 `AgentTool` (孵化新子代理)、`SendMessageTool` (给子代理发消息) 和 `TaskStopTool` (停止出错的子代理) 来调度工作。
- **并发调度 (Fan-out)**: 当遇到诸如“研究一个新 bug”时，它会在一个回复中同时呼叫多个 `AgentTool`：一个负责搜寻前端文件，另一个负责查阅后端数据库逻辑，实现真正意义上的并行研究。

### 3.2 子代理汇报 (Task Notifications)
- 子代理独立在一个虚拟的会话线中运行。当工作完成时，它们会将自身结果封装在一个 `<task-notification>` XML 标签内（包含 `task-id` 和 `status`）。
- 协调者会像收到用户输入一样收到这些异步消息，将其总结，合并上下文后再向终端前的真实人类用户作汇报。
- 如果代理执行失败，协调者还能纠正方向并通过 `SendMessageTool` 给该任务分配新的思路。

---

## 4. Bridge 远程桥接系统 (`src/bridge/`)

`src/bridge/bridgeMain.ts` 实现了一个可以使得在云端 (claude.ai/code) 直接控制本地终端的技术。

- **轮询与长链接**: 采用轮询 (Polling) 与 Server-Sent Events (SSE) 混合模式与 Anthropic 基础架构握手。
- **多实例与 Worktree 隔离**: 远程系统支持 `same-dir` (共享同一目录) 或 `worktree` 模式。在 `worktree` 模式下，系统利用 Git 为每一个新的远程连接创建一个隔离的工作树，避免多人或多会话编辑同一个文件产生的冲突。
- **鉴权与心跳 (Heartbeat)**: 子进程使用从云端分发的短效 JWT Token，Bridge 会监控超时，在距离 Token 过期 5 分钟前通过 `reconnectSession` 向服务器请求刷新。

---

## 5. 工具集合详解 (`src/tools.ts`)

模块通过强类型的 Zod Schema 定义和分发以下类别工具：
- **系统基础**: `BashTool`, `FileEditTool`, `GlobTool`, `FileReadTool`。其中 `FileEditTool` 采用了基于部分片段匹配替换（Snippet Replacement）的策略，而非完全重写，以加速处理并避免 LLM 输出截断导致文件损坏。
- **MCP 动态工具**: 通过 `ListMcpResourcesTool` 和 `ReadMcpResourceTool`。允许开发者在 `.claude/mcp.json` 里定义外部工具（如通过 GitHub、Slack 或公司内网的 API Server）挂载给 Claude 使用。

## 总结
Claude Code 拥有非常超前的工程化设计。它通过深度的 **Context Management (微型压缩+自动记忆)** 和充分挖掘并行计算能力的 **Swarm Coordinator (多代理并发协调)** 架构，展示了现代 Agent CLI 系统如何在算力和物理限制（Token Window）中起舞，是一本极具参考价值的 AI 助手架构教科书。
