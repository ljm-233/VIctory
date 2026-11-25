# 别人的渲染逆向（第三方材料）

`scarlett_render_reverse.zip`（sha256 `6a500235…`）解出来的 19 份文档，原封不动放在这里
当参考。不是我们的分析，也不是运行时证明 —— 作者自己在 `UNRESOLVED.md` 里写明
"静态接口合同，不是运行游戏、PIX capture 或 Windows 实机验证"。

## 和我们实现的关系

**`FORCED_SLOW_PATH.md` 描述的正是我们已经做的事。** 标题内部有一个 GCL 内联编码：快路径
直接往 `+0x18` 的 put 指针写 Xbox 硬件命令包，慢路径才调 `IASetPrimitiveTopology` /
`SetPipelineState` / `DrawIndexedInstanced`。把 GCL 的五个 put/limit 字段
（`+0x18`/`+0x20`/`+0x28`/`+0x30`/`+0x38`）置零，`put < limit` 就永远不成立，全部退回
D3D12 调用。我们的 `crates/victory-gpu/src/list.rs:103` 是 `put: [u64; 5]`，
第 567 行初始化为 `[0; 5]`，`list.rs:2275` 那个测试就叫
`every_inline_command_falls_back_to_the_table`。

## 值得看的几份

- **`EXECUTION_FLOW.md`** —— 启动顺序（direct queue + **四个 compute queue**）、
  `CreatePlacedResourceX1`、初始 GCL 的 `SyncBottomOfPipeX`/`SyncTopOfPipeX` 前导、
  每线程录制的 dirty 合并和 `FlushPendingBarriers` 分组
- **`FAST_PATHS.md`** —— 九类硬件内联路径的字节长度和 opcode（topology 8 字节、
  PSO 28+32、index buffer 12、vertex buffers `4+8*N` 等）
- **`ARCHITECTURE.md`** / **`CLASS_INDEX.md`** —— 对象模型
- **`ADVANCED_RENDER_INTERFACES.md`** —— DXR 和 mesh 的接口面
- **`WATER_SIMULATION_READBACK.md`** —— 水面那块 compute + CPU alias
- **`UNRESOLVED.md`** —— 边界：`DisableDXR=TRUE`、DispatchMesh 走 ExecuteIndirect
  没有直接执行、Query 只有创建没有 Begin/End、Device 126/128-132 和 GCL 144-148
  这些槽位在当前 EXE 里没有活跃调用

## 直接对上的事实

- **`ExecuteIndirect` 5 个签名 9 个包装，DispatchMesh 通过它执行** —— 这解释了
  `VICTORY_INDIRECT_WINDOW=1` 为什么让 `being made` 从 1404 排到 4
- **shader blob 12 字节 `0xDCE7` 头，codec 3 Stored / codec 13 raw DEFLATE** ——
  我们的 native 解压路径
- **688 字节 graphics PSO stream 的 15 个子对象**、`NativePsoInlineData` 32 字节
  （16 prefix + 8 stateAndFlags + 4 tail + 4 shadowOnly）
- **`RageFormatDesc` 12 字节布局，DXGI_FORMAT 直映射**
- **`DisableDXR=TRUE`** —— 标题自己关了 DXR，不用实现
