

**Agentic IDE: OTTE**

产品设计与技术架构文档

基于用户痛点研究的务实设计方案

目标用户：个人开发者 · 平台：Tauri 桌面应用 · 2026年3月

# **第一部分：产品设计**

## **1.1 核心定位**

一款面向个人开发者的独立桌面应用，用于管理多个 Agentic Coding 会话。不是传统 IDE，不是代码编辑器，而是 Agent 编排的控制台。

| 设计原则 做到三个「不」：不重建代码编辑器（用户已有喜欢的 editor），不绑定特定 Agent（Claude Code、Aider、Codex 都应支持），不复制现有工具链（与 git、tmux、docker 集成而非替代）。 |
| :---- |

## **1.2 目标用户画像**

主用户：每天使用 Claude Code / Aider 的个人开发者，同时维护多个项目或多个 feature branch，习惯用 tmux \+ worktree 并行开发，但苦于缺乏可视化管理。可能有本地与远端 VPS 混合使用的场景。

## **1.3 核心功能模块（按优先级排序）**

**P0：必须有的功能（MVP）**

**① 多会话仪表盘**

主界面展示所有活跃 Agent 会话的状态卡片：当前任务描述、运行时长、token 消耗、当前操作（读文件 / 执行命令 / 等待审批）、关联的 git branch 和 worktree 路径。支持一键创建新 session（自动创建 worktree \+ tmux session \+ 启动 agent）。

**② 实时费用追踪**

每个会话的 token 消耗和估算费用实时显示在会话卡片上。全局状态栏显示总费用、本周费用、与订阅额度的比例。可配置费用警报阈值。这是用户第一痛点，社区已独立构建了 7+ 个监控工具证明其紧迫性。

**③ 可视化 Diff 审查**

当 Agent 完成一轮工作后，提供并排 diff 视图（类似 GitHub PR review）。支持逐文件接受/拒绝。提供检查点时间线，可回滚到任意历史点。这解决了「盲目接受变更」的危险问题。

**④ Agent 活动透明日志**

结构化展示 Agent 的每一步操作：读取了哪些文件、搜索了什么、执行了哪些命令、做了什么决策。可过滤、可搜索。解决「Read 3 files —— 哪 3 个？」的透明度问题。

**P1：重要但可延后**

* **任务队列与后台执行：**排队多个任务让 Agent 顺序执行，或火并开 N 个 Agent 并行处理。

* **跨机器会话管理：**通过 Tailscale 连接远端机器上的 Agent 会话，统一管理。

* **Portless 集成：**自动为每个 worktree 分配命名 URL，在 IDE 内嵌入预览窗口。

* **多 Agent 冲突检测：**当多个 Agent 修改了相同文件时提前警告。

**P2：未来愿景**

* **视觉回归审查：**截图对比、浏览器预览（依赖 Agent 能力成熟后再做）。

* **Spec 驱动开发：**内置 requirements.md / tasks.md 编辑器，与 Agent 执行绑定。

* **全局知识库：**跨 session 的 CLAUDE.md 管理和同步。

## **1.4 UI 布局设计**

**版块布局（从左到右）：**

| 区域 | 内容 | 占比 |
| :---- | :---- | :---- |
| 左侧边栏 | Session 列表 \+ 快速操作按钮（新建 / 暂停 / 终止） | \~15% |
| 中央主区 | Agent 交互终端（PTY 渲染）+ Diff 审查视图（可切换） | \~55% |
| 右侧面板 | 活动日志 \+ 费用仪表盘 \+ 文件变更列表 | \~30% |

关键设计决策：Agent 终端占据中央主区的大部分空间，而不是像 Cursor/Windsurf 那样把 Agent 塞进侧边栏。这是从终端 Agent 用户迁移过来的核心 UX 要求。

# **第二部分：技术架构**

## **2.1 整体架构图**

采用 Remote-First C/S 架构，本地客户端轻量无状态，远端守护进程管理一切状态：

| 层次 | 组件 | 职责 |
| :---- | :---- | :---- |
| 客户端 (Tauri) | WebView \+ Rust Core | 渲染 UI、转发用户操作、xterm.js 终端渲染 |
| 网络层 | Tailscale Mesh | 打通本地/远端机器，MagicDNS 命名 |
| 守护进程 | Rust daemon (per machine) | 管理 tmux、worktree、docker sandbox、费用追踪 |
| 会话持久层 | tmux | Session 不死，IDE 断连后自动 reattach |
| 通信层 | node-pty / tmux send-keys | PTY 桥接，模拟键盘输入到交互式 Agent |
| 隔离层 | Git Worktree \+ Docker Sandbox | 代码隔离 \+ 环境隔离 |
| 网络命名 | Portless | 每个 worktree 自动获得命名 URL |
| Agent 运行时 | Claude Code / Aider / Codex | 可插拔，IDE 不绑定特定 Agent |

## **2.2 核心技术选型**

**客户端框架：Tauri**

同意参考文档的结论，但需要补充几个实际考量：

* **Tauri 的优势确实在于内存。**多会话场景下可能同时打开 5–10 个终端渲染器 \+ diff 视图，Electron 的内存压力确实更大。

* **但 Tauri 的 WebView 跨平台一致性是风险。**Windows 用 WebView2，macOS 用 WKWebView，Linux 用 WebKitGTK，渲染行为会有差异。需要投入更多跨平台测试。

* **实际建议：**用 Tauri 2.x，Rust 后端负责所有 tmux/pty/git/docker 交互，前端用 React \+ xterm.js。

**会话持久化：Tmux 自动化**

这是参考文档和我们研究共同确认的最可靠方案。关键设计：

* 每个 Agent session 对应一个 tmux session，命名规则：aide-{project}-{branch}

* IDE 守护进程负责自动创建/销毁 tmux session，与 worktree 生命周期绑定

* 远端机器上的守护进程通过 Tailscale SSH 无密码连接，客户端断开后 tmux session 继续运行

* 客户端重连时自动 reattach，无感恢复

**交互式数据注入：PTY 桥接**

这是整个架构中技术难度最高的部分。参考文档对问题的分析是准确的，但需要补充分层策略：

**短期方案（MVP）：tmux send-keys \+ capture-pane**

* IDE 通过 tmux send-keys \-t session-name "prompt" Enter 向交互式 Agent 注入提示词

* 通过 tmux capture-pane \-t session-name \-p 读取 Agent 的输出

* 优点：完全不依赖 Agent 的内部实现，任何基于终端的 Agent 都支持

* 缺点：输出解析需要处理 ANSI 转义序列，无法获取结构化数据

**中期方案：PTY 直接桥接**

* 用 Rust 的 portable-pty 或 Node 的 node-pty 分配伪终端

* Agent 运行在 pty 中，认为自己连接真实终端，完整保留交互模式能力

* IDE 通过 pty master 端读写，同时用 xterm.js 渲染给用户

* 优点：实时双向通信，渲染保真度高

* 缺点：需要处理 ANSI 解析、流速控制、终端尺寸同步

**长期方案：Agent SDK 直接集成**

* 直接用 Claude Code Agent SDK 构建自己的交互循环

* 获得结构化输出、tool 审批回调、原生消息对象

* 完全控制 Agent 的行为，不再依赖终端解析

* 缺点：开发量大，且每个 Agent 需要单独适配

| 核心判断 MVP 用 tmux send-keys 先跑起来，因为它对所有 Agent 都通用。中期过渡到 PTY 桥接以获得更好的实时性。长期对最常用的 Agent（如 Claude Code）做 SDK 直接集成。三层可以并存，通过 Agent 适配器模式统一接口。 |
| :---- |

**环境隔离：Docker Sandbox 为主，裸 Worktree 为辅**

参考文档提到了 Nix Flakes、Docker、Firecracker 三个选项。我们的判断：

| 方案 | 适用场景 | 不适用场景 |
| :---- | :---- | :---- |
| 裸 Git Worktree | 轻量项目、不需要环境隔离的场景 | 多服务、端口冲突、系统级依赖差异 |
| Docker Sandbox | 需要环境隔离、想用 \--dangerously-skip-permissions | 资源受限的机器、不想装 Docker 的用户 |
| Nix Flakes | 不推荐用于 MVP | 学习曲线太陡，缩小目标用户群 |
| Firecracker | 不推荐用于 MVP | 复杂度太高，个人开发者不需要内核级隔离 |

策略：默认用裸 worktree（零开销），用户可选开启 Docker Sandbox 隔离。通过 Environment Provider 抽象层统一接口，未来可扩展其他后端。

**网络层：Tailscale \+ Portless**

参考文档的分析是准确的。补充一个关键实现细节：

* Portless 目前只解析到 localhost，跨机器访问需要额外处理

* 方案：在远端机器上用 Tailscale Serve 把 portless 的本地地址暴露到 tailnet

* 效果：开发者在任何机器上访问 feature-auth.vps.tailnet 即可预览对应 worktree 的服务

# **第三部分：Session 生命周期**

## **3.1 创建 Session 的自动化流程**

用户在 IDE 点击「新建 Session」时，指定目标机器、项目、分支和 Agent 类型，剩下全部自动化：

1. **环境准备：**在目标机器上创建 git worktree（如需要，同时创建 Docker Sandbox）

2. **Session 持久化：**创建命名 tmux session，工作目录设为 worktree 路径

3. **Agent 启动：**在 tmux session 内启动指定的 coding agent

4. **网络配置：**注册 portless 命名（如需要，配置 Tailscale Serve）

5. **通信建立：**建立 IDE ↔ tmux session 的 PTY/send-keys 通道

6. **状态注册：**向 IDE 仪表盘注册新 session，开始监控

## **3.2 Session 状态机**

| 状态 | 含义 | 转换触发 |
| :---- | :---- | :---- |
| Creating | 正在创建 worktree/sandbox/tmux | 用户点击「新建」 |
| Running | Agent 正在执行任务 | 创建完成 |
| Waiting | Agent 在等待用户输入或审批 | Agent 询问 / 权限请求 |
| Paused | 用户主动暂停 | 用户点击「暂停」 |
| Disconnected | IDE 与 tmux 断连，Agent 仍在运行 | SSH/网络中断 |
| Completed | Agent 任务完成 | Agent 退出 |
| Error | Agent 崩溃或异常退出 | 进程异常 |

关键设计：Disconnected 状态是“会话不死”的核心。用户合上笔记本回家，Agent 在远端 VPS 的 tmux 里继续工作。重新打开 IDE 时自动重连，状态无缝恢复。

# **第四部分：竞争壁垒与差异化**

## **4.1 与现有方案的差异**

| 维度 | 本产品 | 现有工具（Claude Squad/Codeman/等） | IDE Agent（Cursor/Windsurf） |
| :---- | :---- | :---- | :---- |
| 多会话管理 | 原生可视化仪表盘 | CLI 或简单 WebUI | 单会话侧边栏 |
| Agent 无关性 | 支持任意终端 Agent | 通常绑定特定 Agent | 绑定自家模型 |
| 费用追踪 | 内置一等功能 | 需要第三方工具 | 无或极简 |
| 环境隔离 | Worktree \+ Docker Sandbox | 仅 Worktree | 无显式隔离 |
| 远程支持 | Tailscale 原生支持 | 需手动 SSH | 部分支持 |
| 变更审查 | PR 级别的 diff 视图 | 无 | 内联 diff |

## **4.2 核心竞争壁垒**

**「可插拔 Agent」是最大的差异化点。**Cursor 绑定自家模型，Windsurf 绑定 Codeium，而我们的 IDE 让用户自由选择每个任务用哪个 Agent。当模型能力快速迭代时，不绑定特定供应商是最大的用户价值。

**「会话不死」是次要但很实用的壁垒。**通过 tmux \+ Tailscale 实现的跨机器会话持久化，让开发者真正做到 fire-and-forget。

# **第五部分：MVP 范围与路线图**

## **5.1 MVP（v0.1）—— 4–6 周**

* 本地多会话仪表盘（创建/停止/切换）

* 自动 worktree \+ tmux session 创建

* PTY 终端渲染（xterm.js）

* tmux send-keys 向 Agent 注入提示词

* 基础费用追踪（读取 Claude Code 的本地 usage 数据）

* Git diff 查看器

## **5.2 v0.2 —— \+4 周**

* Tailscale 远程机器支持

* Docker Sandbox 集成

* Portless 命名 \+ 内嵌预览窗口

* 完整的检查点/回滚系统

* 多 Agent 支持（Aider、Codex CLI）

## **5.3 v0.3 —— \+4 周**

* 任务队列和并行编排

* 多 Agent 冲突检测

* Agent SDK 直接集成（Claude Code）

* 中国市场支持（多模型后端、中转站兼容）

# **结语**

参考文档的核心框架是正确的：Agent 为主的界面、分层隔离、tmux 持久化、PTY 桥接、Portless \+ Tailscale 网络。但它在实战层面有过度设计的问题，且遗漏了 Claude Code Remote Control 等重要的新发展。

本文档的核心调整是：

7. **砍掉过度设计：**全局记忆浏览器、视觉回归引擎、策略冲突总线等降为 P2 愿景，MVP 专注于已验证的高频痛点。

8. **补充实现细节：**为每个技术选型给出分层策略（短期/中期/长期）而非一步到位。

9. **强调 Agent 无关性：**这是竞争壁垒的核心，也是参考文档未充分展开的点。

10. **明确 MVP 范围：**4–6 周可交付的最小可用产品，而非一个宏大的愿景文档。

**一句话总结：**这款 IDE 不是另一个 Cursor，而是「终端 Agent 用户的下一步」——保留他们喜欢的 Agent 的全部力量，加上他们一直缺少的编排、审查和可见性。