# Claude Code 源码 Rust 重构指南与架构映射

将 Claude Code (约 50 万行 TypeScript + Bun) 重构为 Rust 是一个极具挑战性但收益巨大的工程。Rust 能够带来无垃圾回收 (GC) 的确定性内存控制、极致的启动速度（CLI 工具的核心诉求）、极低的基础内存占用，以及无畏并发（Fearless Concurrency）。

以下是完整的重构思路、生态映射与渐进式重构方案。

---

## 1. 技术栈与生态映射 (TypeScript/Bun -> Rust)

| 核心领域 | TypeScript / Bun 生态 | Rust 生态推荐 | 理由/对比 |
| :--- | :--- | :--- | :--- |
| **CLI 解析** | `Commander.js` | **`clap`** (加上 `derive` 宏) | 业界标准，支持强类型的命令行参数解析，完美替代 Commander。 |
| **运行时 & 并发** | Bun (Event Loop) | **`tokio`** | 异步运行时的绝对主宰，处理长连接、WebSocket (用于 Bridge) 和并发子任务。 |
| **终端 UI 渲染** | `React` + `Ink` | **`ratatui`** + **`crossterm`** | Ratatui 提供基于即时模式 (Immediate Mode) 的 UI 渲染。虽然没有 React 声明式那么直观，但性能极佳且是 Rust TUI 标配。 |
| **数据验证与序列化**| `Zod` | **`serde`** + **`serde_json`** | Rust 的 Serde 性能极佳，利用编译期宏展开，通过强类型完全取代 Zod 在运行时的耗时校验。 |
| **HTTP 客户端** | Fetch (Bun 内置) / Axios| **`reqwest`** / **`hyper`** | 用于与 Anthropic API 及 MCP 远端服务端交互。支持流式读取 (Streams)。 |
| **子进程调用** | `Bun.spawn` | **`tokio::process`** | 处理 `BashTool` 的命令执行，具备异步非阻塞 I/O 的标准实现。 |
| **全双工/RPC** | `WebSocket` (Bridge) | **`tokio-tungstenite`** | 完美处理跨进程及 Remote Control 守护进程的数据交互。 |
| **文件匹配搜索** | `Glob` / `ripgrep` | **`ignore`** + **`grep-regex`** (BurntSushi 生态) | Rust 拥有原生的高性能搜索库（`ripgrep` 就是 Rust 写的），可以直接集成依赖而不需要开子进程。 |

---

## 2. 核心架构转换策略

### 2.1 接口 (Interfaces) 转换为特征 (Traits)
在 TS 中，`Tool` 和 `Command` 是充满可选方法（Optional Methods）的接口。在 Rust 中，这应重构为带有默认实现的 `Trait`。

**TS 代码:**
```typescript
export interface Tool<Input, Output> {
    name: string;
    description(input: Input): Promise<string>;
    call(input: Input, ctx: ToolUseContext): Promise<ToolResult<Output>>;
    isDestructive?(): boolean; // Optional
}
```

**Rust 映射:**
```rust
#[async_trait]
pub trait Tool {
    type Input: DeserializeOwned;
    type Output: Serialize;

    fn name(&self) -> &'static str;

    async fn description(&self, input: &Self::Input) -> Result<String, Error>;

    async fn call(
        &self,
        input: Self::Input,
        ctx: &mut ToolUseContext
    ) -> Result<ToolResult<Self::Output>, Error>;

    // 使用默认实现替代 TS 的 Optional
    fn is_destructive(&self) -> bool { false }
}
```
*注：由于涉及到动态分发（Dynamic Dispatch），可能需要使用 `Box<dyn Tool>`，此时需要处理好 `async_trait` 或 Rust 1.75 之后的原生 Async Traits 在 `dyn` 下的支持。*

### 2.2 状态管理与并发控制
TypeScript 中的单线程异步使得操作 `MutableMessages` (对话历史) 非常容易。在 Rust 中，多线程 Tokio 运行时需要我们更严谨地对待所有权。
- **对话历史 `AppState`**: 使用 `Arc<RwLock<AppState>>` 或使用 Actor 模式，通过 `mpsc::channel` 把状态更新的消息发送给一个中央的 Coordinator 线程，以避免锁竞争。
- **QueryEngine 的 Async Generator**: TS 中广泛使用了 `async function* query()` 来抛出 (yield) UI 渲染事件和模型回答片段。在 Rust 中，应映射为实现 `Stream` trait 或使用 `tokio::sync::mpsc` 的 Receiver 端：

  ```rust
  pub async fn query_engine(
      mut rx: mpsc::Receiver<UserAction>,
      tx: mpsc::Sender<StreamEvent>
  ) {
      while let Some(action) = rx.recv().await {
          // 调用模型 API 并向 tx 流式发送片段
          // tx.send(StreamEvent::ContentBlockDelta(...)).await;
      }
  }
  ```

### 2.3 TUI 的重构 (Ink 到 Ratatui)
Claude Code 的界面非常复杂，它利用了 React 的 VDOM 进行局部刷新。
- **重构挑战**: Ratatui 是即时渲染的。你需要在每一帧 (Tick) 重新计算所有的 `Layout` 和 `Block`。
- **解决方案**: 构建一套 Model-View-Update (MVU) 架构，类似于 Elm。主应用持有一个 `App` 结构体，包含当前所有终端内容、进度条动画的帧索引等，在每次按键或收到 `QueryEngine` 的 `StreamEvent` 时更新 `App` 结构，再调用 `terminal.draw(|f| ui(f, &app))?`。

### 2.4 MCP & Plugins 的动态解析
TS 可以动态 `import()` 或 `require()` 本地脚本。Rust 是静态编译语言，不能在运行时加载 `.ts`/`.js` 文件。
- **解决方案**:
  - 对于 MCP (Model Context Protocol) 依然通过 JSON-RPC 标准的 `stdio` 或 WebSocket 子进程进行跨语言通信，这点和目前毫无区别。
  - 对于本地的 `.claude/skills/` 脚本，可以集成 **`Rhai`**，**`mlua`** 或 **`deno_core`** (v8) 作为一个小巧的沙箱，让 Rust 在运行时能执行那些自定义技能脚本。

---

## 3. 渐进式重构路线图 (Gradual Migration Plan)

重构 50 万行代码不可能一蹴而就，建议采用**基于 FFI (Foreign Function Interface) 的绞杀者无花果 (Strangler Fig) 模式**。

### 第一阶段：重写热点 (Hot-paths) 与核心工具
在现有的 Bun 应用中，使用 N-API (通过 `napi-rs`) 将性能敏感、系统级操作模块化为 Rust 库。
- 将 `src/tools/GlobTool/` 和 `GrepTool` 彻底移除，换成通过 `napi-rs` 包装的 Rust 高性能本地搜索模块。
- 使用 Rust 重构文件系统的 `FileEditTool`，特别是实现快速的大文件 AST 提取或行级别的补丁覆盖 (Snippet replacement)。

### 第二阶段：重构模型引擎与隔离 TUI
- 将核心的 `QueryEngine`、`claude.ts`（API 交互、Token 计算和流解析）及 `AutoCompact` 逻辑用 Rust 重写，编译成一个内部微服务或动态链接库。
- Bun 这一侧的 React/Ink UI 只负责渲染，通过 IPC 或 FFI 接收 Rust 引擎传来的 `StreamEvent`。

### 第三阶段：剥离 Node.js/Bun，构建独立的 Rust CLI
- 使用 `ratatui` 重构 UI，并与第二阶段写好的引擎直连。
- 用 `clap` 重写命令解析 `src/commands.ts`。
- 实现原生的 Remote Bridge (原 `src/bridge/bridgeMain.ts`)：利用 `tokio-tungstenite` 接收来自外部的远程操作指令，并隔离管理多个 Agent 工作区 (Worktree)。
- 彻底抛弃 TS 运行时，分发单一平台静态二进制文件（零外部依赖）。

---

## 4. 重点难点分析
1. **Prompt Cache 与序列化一致性**: Anthropic API 对指纹和 JSON 的顺序极端敏感。在将 Zod 迁移到 Serde 时，需严格保证对象序列化时的字段顺序 (`preserve_order` features)，否则可能导致云端的 `cache_control` 完全失效，成本飙升。
2. **Ink 到 Ratatui 的视觉保真度**: Claude Code 有大量精致的微动画、Spinner、和文本高亮 (Syntax Highlighting)。在 Ratatui 中需要借助 `syntect` 进行代码染色，并自行维护定时器来推进 Spinner 状态。
3. **Agent Swarm 通信**: TypeScript 中的 Event Loop 能轻易完成多子 Agent 的异步协同。Rust 中需仔细设计 Actor 模型，使用多 Sender / 单 Receiver 队列将子 Agent 的任务进度 (`<task-notification>`) 回传给主 Coordinator，并小心防止死锁或跨线程 Send 失败。

Rust 会让 Claude Code 成为体积最小 (可能从 Bun 的数百 MB 降至 30MB 以下的二进制)、最快、完全便携且安全的代码助手，但这需要优秀的并发设计与架构抽象才能实现。
