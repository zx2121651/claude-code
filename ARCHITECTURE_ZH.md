# Claude Code 深度源码与架构分析报告

本报告对 Anthropic 发布的终端 AI 编程助手工具 **Claude Code** 的核心源码与运行架构进行模块化的深入分析。整个项目包含超过 50 万行代码，由 TypeScript 编写，运行在 Bun 环境下，并具有丰富的多代理（Swarm）协作与上下文压缩管理能力。

## 1. 系统核心生命周期 (`src/main.tsx` & `src/QueryEngine.ts`)

### 1.1 启动与初始化 (`src/main.tsx`)
`main.tsx` 是 CLI 的主入口。程序启动时极度注重性能：
- **并行预加载 (Parallel Prefetching)**: 在所有模块解析之前，系统立即发起对 MDM (设备管理策略) 的异步查询和从 macOS Keychain 中读取授权凭证。
- **动态模块加载 (Lazy Loading)**: 将体积庞大的监控、分析库（如 OpenTelemetry、gRPC 等）通过动态 `import()` 延后至界面渲染之后加载，保证 TUI 能够秒开。
- **环境嗅探与 CLI 解析**: 解析诸如 `--model`, `--print`, `--mcp-config` 等参数，并配置基础的环境变量（如隔离沙箱标志、进程标题等）。

### 1.2 会话引擎 (`src/QueryEngine.ts`)
负责处理核心的问答（Query）生命周期：
- 维护每一个 `Turn` 的状态（包含对话历史、正在执行的工具、剩余预算 `maxBudgetUsd`）。
- **拦截与恢复**: 接收底层 `claude.ts` 抛出的如 `max_output_tokens` (长度截断)、`413 Payload Too Large` 等流式错误，将其转交给压缩器 (Context Collapse) 清理历史冗余，然后原路重试（Fallback Retry）。
- **流式追踪与控制**: 基于 `AbortController` 监听用户 `Ctrl+C` 事件中止大模型的请求，但保留已执行的工具产生的附加上下文。

## 2. 大模型 API 封装与重试 (`src/services/api/claude.ts`)
- **流事件解析 (SSE Parsing)**: 原生捕获 `message_start`, `content_block_delta` 和 `message_stop`，将不确定的 `tool_use` JSON 片段实时组装。
- **Prompt Caching 优化**: 自动向 `system_prompt` 和大段历史工具结果（`tool_result`）标记 `cache_control: { type: "ephemeral" }`。引擎还精妙地通过 `cache_edits` 指令（Cached Microcompact）来只剔除模型远端缓存中的部分无用历史，省去重复上传全量上下文的巨额 Token 花费。
- **自动降级 (Fallback)**: 当首选模型（如 Opus）触发服务过载时，系统根据预设平滑降级（如 Sonnet），并通过重新提交剔除原有特化标签（如特化思考块）的 Payload 进行重试。

## 3. 上下文与 Token 管理 (`src/services/compact/`)
由于代码库操作的 Token 会迅速爆炸，系统设计了多层防御机制：
- **MicroCompact (微型压缩)**: 发生在一轮问答中。大模型如果用 `ls -la` 读取了一个超长目录，或者引发大量控制台错误，系统会在下一次提问前总结这些内容，将详细记录剔除（同时通知服务端 `cache_deleted_input_tokens`）。
- **Context Collapse (上下文折叠)**: 针对只读命令 (如 `grep`, `cat`)，如果模型尝试多次仍未找到所需信息，系统折叠这些无用的探索步骤为一句摘要，释放近 90% 的探索 Token。
- **AutoCompact (自动会话压缩)**: 当总会话 Token 逼近物理上限（如 200k）时，系统自动挂起（Suspend）当前查询。单开一个 Agent 把整个历史提炼成一篇包含任务进度和关键发现的摘要文档，然后丢弃前面的对话，带着摘要从头开始。
- **ReactiveCompact (反应性压缩)**: 在 API 请求直接遭到 `413` 拒绝时的最后一道防线。它强行截断部分长图、大型附件并执行紧急压缩后再尝试发送。

## 4. 多代理协作机制 (Agent Swarm & Coordinator)
在 `src/coordinator/` 与 `src/tools/AgentTool/` 下，实现了一套“主从架构”（Swarm）：
- **协调者 (Coordinator Mode)**: Coordinator 不亲自动手读取文件，而是作为“包工头”使用 `AgentTool` (孵化)、`SendMessageTool` (跟进)、`TaskStopTool` (停止) 工具。
- **并发调度 (Fan-out)**: 当用户询问一个大型的复杂需求时，协调者会并发孵化 2~3 个独立的 worker-agent 去各自负责独立的文件路径。
- **异步汇报 (`TaskNotification`)**: 工作完成后，worker 会发送格式化的 `<task-notification>` XML 返回给主干线，携带自身是否成功、消耗时长及结论摘要，由 Coordinator 合成后向用户反馈。

## 5. 工具集合架构 (`src/tools.ts`, `src/Tool.ts`)
所有能力都被封装成了统一的 `Tool` 接口。
- **内部基础工具**:
  - `BashTool` (执行终端)、`FileEditTool` (精准行替换或 AST 替换)、`GlobTool` & `GrepTool` (代码库扫描)。
  - 各种读写、控制代理生命周期的原生工具。
- **动态寻址 (Tool Search)**: 为防止首轮载入数十个工具耗尽上下文，系统运用 `ToolSearchTool` 让模型自行“发现”被标记为 `shouldDefer` 的低频工具。
- **输入输出流拦截**: `BackfillObservableInput` 允许工具在发往后台和写入本地历史记录之前，对 JSON 参数进行补充和变形。

## 6. MCP & 插件/技能 (Plugins & Skills)
- **MCP 客户端 (`src/services/mcp/`)**: 实现对 Model Context Protocol 标准的支持。在解析完用户的 `--mcp-config` 后，建立与外置工具（如 GitHub/Jira/数据库）的 WebSocket/Stdio 通信，模型从而能无缝接入企业内部网服务。
- **Skills (`src/skills/`) & Plugins (`src/plugins/`)**: 用于存放基于自然语言或预设命令的重用脚手架脚本和快捷指令（如 `/review`, `/commit`）。通过读取 `.claude/skills/` 下的 markdown，自动转化为能够被识别和分发的工具能力。

## 7. 远程桥接与隔离 (`src/bridge/bridgeMain.ts`)
使得用户在浏览器 (`claude.ai/code`) 中直接编写和调试本地代码的底层机制：
- **安全与长链接**: 本地 CLI 作为 Daemon 运行，通过长链接与 Anthropic 服务器握手注册 `environment_id`，并响应服务器推送的工作指令。
- **工作区隔离 (Worktree Mode)**: 当设置 `--spawn=worktree` 时，多个网络请求将通过 Git 创建相互隔离的 Worktree 目录，使得远程多个并发子 Agent 不会串写修改。

## 8. 永久记忆与长期规划 (`src/memdir/`)
为使得模型具备跨会话的记忆力：
- 系统读取和管理 `.claude/` 中的记忆索引文件。在每轮对话中，基于模型的操作线索前置预取 (Prefetch) 相关的知识记录。
- **记忆快照 (Memory Snapshot)**: 当一个大型 Agent 执行完毕时，会主动进行记忆融合和覆写，更新项目上下文的理解，供下一次启动使用。

## 9. 渲染引擎 (`src/ink/`)
- 基于 React 的 Terminal 渲染器，对 `Ink` 框架进行了大量性能和定制化修改。
- **布局流**: 支持 Flexbox、文字换行、ANSI 转义渲染，并提供对鼠标交互（Hit-Test）和超链接（Hyperlinks）的支持。它能够动态重绘工具执行时的微进度条 (Spinner/Progress) 和终端弹窗。
