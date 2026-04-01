# Claude Code 源码与架构分析

## 1. 项目概述
Claude Code 是 Anthropic 官方提供的一个基于终端 (Terminal) 的 AI 编程助手命令行工具（CLI）。它允许开发者在终端中通过自然语言与大语言模型 (LLM) 进行交互，并授权模型执行各种软件工程任务，如读写文件、运行终端命令、搜索代码库、发起网络请求，甚至协调多个子代理 (Sub-agents) 协同工作。

项目主要使用 **TypeScript** 编写，运行在 **Bun** 环境下，并使用了 **React** 和 **Ink** 库来构建终端用户界面 (TUI)。整个项目包含约 1900 个文件和超过 50 万行代码。

## 2. 核心架构与模块

项目代码主要集中在 `src/` 目录下，架构可以分为以下几个核心部分：

### 2.1 核心入口与生命周期 (`src/main.tsx` & `src/QueryEngine.ts`)
* **`main.tsx`**: CLI 的主入口。使用 `Commander.js` 解析命令行参数。在启动阶段，它会进行大量的性能优化（如并行预取 MDM 策略、Keychain 凭证、以及连接 Anthropic API），随后初始化配置、鉴权状态和遥迹 (Telemetry) 模块。接着会渲染基于 Ink 的终端 UI 界面，并将控制权交给内部的交互循环。
* **`QueryEngine.ts`**: 负责处理 LLM 查询的生命周期与会话状态。它管理着每一轮 (turn) 的对话，包括调用模型 API（带流式输出）、处理模型返回的工具调用块 (Tool Use Blocks)、中断处理（如 `AbortController` 触发的取消）、以及发生错误（如 `max_output_tokens` 或 413 错误）时的上下文折叠（Context Collapse）或重试机制。

### 2.2 工具系统 (`src/tools/` & `src/Tool.ts`)
Claude Code 最强大的能力来自于其模块化的工具系统。每个工具都实现了统一的 `Tool` 接口，定义了其输入校验（Zod schema）、权限模型和执行逻辑。
* **主要内置工具**:
  * `BashTool`: 执行终端 Shell 命令。
  * `FileReadTool` / `FileEditTool` / `FileWriteTool`: 文件的读取、局部修改和覆盖写入。
  * `GlobTool` / `GrepTool`: 本地文件匹配和基于 `ripgrep` 的内容搜索。
  * `AgentTool`: 孵化并管理子代理，支持多代理协作（Swarm 架构）。
  * `MCPTool` / `LSPTool`: Model Context Protocol (MCP) 服务器和语言服务器 (LSP) 集成。
* 工具执行时，引擎会通过 `canUseTool` 钩子检查权限，提示用户确认或者根据权限模式 (Permission Mode) 自动放行。

### 2.3 命令系统 (`src/commands/` & `src/commands.ts`)
命令系统处理用户在对话框中输入的以 `/` 开头的斜杠命令。
* 包括大约 50 个命令，例如：
  * `/compact`: 手动压缩上下文。
  * `/config`: 管理系统配置。
  * `/review` / `/commit`: 代码审查与 Git 提交。
  * `/mcp`: 管理和配置 MCP 服务器。

### 2.4 API 与网络服务 (`src/services/` & `src/utils/api/claude.ts`)
* **API 交互**: `claude.ts` 封装了对 Anthropic API 的调用，包含 Beta 头（如缓存控制、结构化输出等）的组装、API 失败时的非流式降级和重试 (`withRetry`)。
* **Prompt 缓存**: 工具大量利用了 API 的 Prompt Caching 特性。代码中精心排列了系统提示 (System Prompt) 和工具模式 (Tool Schemas)，在长上下文中放置 `cache_control` 标记以最大化缓存命中率，降低成本。
* **MCP (Model Context Protocol) 客户端**: `src/services/mcp/` 负责管理外接的 MCP 服务器，允许模型动态获取来自第三方系统的工具和资源。

### 2.5 上下文管理与压缩 (`src/services/compact/`)
由于代码库操作和命令执行容易迅速消耗大量上下文 Token，系统实现了复杂的上下文管理策略：
* **Auto Compact (自动压缩)**: 当 Token 数量达到阈值时，系统会自动将之前的对话历史总结为压缩的记忆，释放上下文空间。
* **Micro Compact / Context Collapse**: 在遇到大结果或者 Token 超限时，会自动剔除部分冗余信息（如大段的读文件结果或图片）。

## 3. 安全与权限管理
* **沙箱与权限 (`src/utils/permissions/`)**: 系统具有细粒度的权限控制。在执行高风险工具（如 `BashTool`、`FileWriteTool`）时，需要基于用户的权限模式 (`default`, `auto`, `plan` 等) 决定是否弹窗请求批准。
* 对于被企业管理 (MDM) 的设备，可以通过远程配置禁用某些命令和 MCP 服务器。

## 4. 总结
Claude Code 是一个高度工程化和智能化的 Agent 系统。其底层通过精心设计的并发架构缩短启动时间，中层依靠 Zod 确保工具调用的强类型与安全，上层结合 Ink 实现了美观流畅的交互，并在最核心的 API 层通过重试、缓存和动态 Token 管理，榨干了 LLM（特别是 Claude 3.5/3.7 系列）在代码协助上的潜力。
