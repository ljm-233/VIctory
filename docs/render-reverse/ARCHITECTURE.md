# 渲染架构与内部状态

## 1. 逻辑对象而非虚构的原始类名

当前 EXE 大量渲染状态位于全局数组和线程局部大块内存中，并非所有功能都对应一个 C++ 类。因此本目录使用两类定义：

1. **ABI 结构**：字段偏移、步长和类型有汇编证据，例如 `SubmissionRecord`。
2. **逻辑类**：把同一职责的全局量、TLS 字段和函数组织成 C++ 类，便于理解，例如 `PlatformRenderDevice`。逻辑类不宣称是原源码类名，也不把其 `sizeof` 当作原对象大小。

这种区分很重要：把全局数组直接包装成类只是表达所有权和流程，不会据此虚构原程序的继承关系。

## 2. 平台设备层

`Platform_GraphicsBoot`（`0x1419F4DF0`）先准备渲染器全局状态和窗口配置，再进入 `Platform_CreateWindowAndD3D12Device`（`0x142478440`）。后者是多个子系统的汇合点，而不是单纯 `CreateDevice` 包装：

- Win32 窗口和消息处理环境。
- 一个 `ID3D12Device11`，同一指针被发布给多个渲染子系统。
- 一个 direct queue，用于主图形提交和 Present。
- 四个 compute queue，各自关联独立的队列上下文。
- direct/compute 命令列表池。
- descriptor heap、根签名、PSO 管理和 DXR 辅助对象。
- 帧事件、帧统计和显示配置。

初始化中创建的 GCL 先写入 bottom-of-pipe/top-of-pipe 同步前导再 `Close`。这意味着池中的“空列表”并非零字节列表，而是带有引擎要求的 cache/pipe 同步序列；后续 `ResetX` 是否重放前导要结合获取路径理解，不能只看创建函数。

### 队列与列表索引不是同一概念

- queue index 选择 direct 或某个 compute queue。
- pooled list index 选择 `qword_1475E7300[]` 中的 GCL。
- 单列表提交包装把一个字节索引同时用于池索引。当前创建路径中 0 是 direct，1..4 对应 compute；5..10 只观察到保留与 shutdown 释放语义，没有实际创建或提交合同。

伪代码使用 `CommandListSlot` 和 `QueueContextId` 两种类型，避免把索引语义混成一个整数。

## 3. 每线程渲染上下文

`0x140188430` 是线程命令上下文的懒初始化入口。调用者通常检查线程大块内存中的 GCL 指针是否为空，为空才进入此函数。它完成：

1. 在线程上下文中建立与 queue context 的关联。
2. 尝试从 direct 池获取命令列表；不适配时走 compute/专用池。
3. 保存 GCL、allocator 句柄/池节点及所属池。
4. 建立“当前命令段”的写入状态。

此后绘制函数有两种编码路径：

- **快路径**：当前命令段有足够空间时，直接写硬件/驱动命令字并推进 write cursor。
- **慢路径**：空间不足或状态复杂时，通过对象函数表调用对应 XS/包装方法。

因此只 Hook `ID3D12GraphicsCommandList6` 函数表不能截获全部绘制。完整渲染替换必须同时处理内联编码器、命令段所有权和池生命周期。

## 4. 绘制状态层

`0x14248CE00` 展示绘制前的状态组织：

- 当前 dirty 位与既有位合并，形成一次绘制需要处理的状态集合。
- 特殊拦截状态可触发回调，并伴随引用/递归计数变化。
- 复杂状态进入 `0x140032520`，root 子集进入 `0x140032130`。
- 状态清理后，如果存在当前 PSO/程序状态，就把一个 24 字节绘制请求直接写入命令段；空间不足才走函数表。

这里恢复的类会显式区分：

```text
RequestedState       游戏线程希望使用的状态
DirtyState           与已编码状态比较后仍需处理的部分
EncodedState         已进入当前命令段的状态
RootBindingCache     root descriptor/constant 的缓存
PipelineState        当前可用于 Draw 的 PSO/内部程序对象
CommandSegment       当前写指针、结束指针及段标志
```

这些不是同一份状态。把 dirty flag 清除当作“GPU 已执行”是错误的；它只表示状态已编码，GPU 完成仍需队列/fence。

## 5. 异步提交层

生产者 `0x1401F6330` 和消费者 `0x142481E90` 共享 96 个批次。每批头为 72 字节，记录区由外部指针和容量描述。记录固定 80 字节，类型由首字节决定。

| 类型 | 当前命名 | 作用 |
|---:|---|---|
| 0 | `SubmitCommandList` | 提供一个 GCL、提交后回调对象及冲刷控制；批量 Execute 后回调被直接调用 |
| 1 | `InvokeCallback` | 在提交顺序中调用 CPU 回调 |
| 2 | `Present` | 提供一个或多个内部 plane、显示配置和 frame-tracking 信息 |
| 3 | `FrameToken` | 设置当前 token，或等待/取得一个新 token |

消费者不是遇到一条 Submit 就立即调用 XS。它累积 GCL，在记录边界、Present、token 或容量限制处批量提交；一批最多收集 256 个列表。批量提交使用 `NO_SWITCH_BUFFER | CHAIN_SEGMENTS`，单列表包装使用 `NONE`。每条 Submit 的完成回调在该次批量 Execute 后按收集顺序执行。

### token 与 fence 不是同一东西

- `D3D12XBOX_FRAME_PIPELINE_TOKEN` 把提交和 Present 归属到帧管线。
- `ID3D12Fence` 的值表示队列已经完成到某个 CPU 可见同步点。
- 批次还可携带完成状态指针和事件/等待计数，供其他线程回收资源。

把 token 当 fence value，或把 `Signal` 当 Present，会破坏生命周期模型。

## 6. Present 的两套 plane 结构

内部 Present 记录中的 plane 输入每项 24 字节；消费者再构造 XS 的 `D3D12XBOX_PRESENT_PLANE_PARAMETERS`，每项 `0x48` 字节、8 字节对齐。二者不能混用：

```text
PresentRecord.planes[2]              // 内联在 80 字节记录中，每项 24 字节
        │ 展开资源、源矩形、色彩空间
        ▼
D3D12XBOX_PRESENT_PLANE_PARAMETERS[] // XS ABI，每项 0x48 字节
        │
        ▼
queue->PresentX(...)
```

每个输出 plane 的 `Token` 来自消费者当前 token，`ResourceCount=1`。源矩形为 `{0,0,width,height}`，所有 plane 共用归一化目标矩形 `{0,0,1,1}`；scale filter、扩展描述和 plane flags 都使用默认零值。`PresentParameters` 同样零初始化，只有请求 Present 间隔为 0 时设置 `IMMEDIATE`。

## 7. 帧推进与回收

`Platform_FrameStep`（`0x1424846D0`）负责消息泵、视图选择、Present 入队、帧推进和两类延迟对象链回收。`Platform_AdvanceFrameIndex`（`0x142474740`）维护：

- 两个帧对象槽，按 `index & 1` 交替写入。
- 产生/消费序号，旧槽在不再位于当前保留窗口时释放。
- 视图资源数组的循环索引。
- 4096 项回调/对象环，索引以 12 位回绕。

回收算法和提交 fence 协同，但“数组槽被覆盖”本身不是 GPU 完成证明。伪代码把“帧索引推进”“CPU 延迟链处理”“GPU fence 完成”分开描述。

## 8. 资源与内存层

已确认资源路径同时存在：

- `CreateCommittedResource3`：运行库根据 heap 属性负责资源存储。
- `CreatePlacedResourceX1`：资源对象放到调用者提供的 GPU VA。
- page pool / physical page mapping：页注册、映射和别名管理。
- `XGCreateTextureComputer`：布局、 mip/slice 和格式相关的纹理初始化计算。
- descriptor heap 与 placed/raw SRV/UAV。

资源章节围绕所有权展开：`GpuMemoryAllocation` 拥有地址/页，`ID3D12Resource` 是视图对象，`Descriptor` 是 shader 绑定视图，三者生命周期不同。具体实现见 `03_resource_memory.cpp` 与 `04_descriptors_copy.cpp`。

## 9. 正常一帧的因果顺序

```text
渲染线程
  懒获取线程命令上下文
  → 设置材质/资源/PSO，累积 dirty 状态
  → 刷新状态并内联编码 Draw/Dispatch/Copy
  → Close / 生成 Submit 记录

平台帧线程
  选择本帧视图资源
  → 生成 FrameToken / Present 记录
  → 推进 CPU 帧索引和延迟回收环

提交消费者
  聚合 Submit
  → ExecuteCommandLists2X(currentToken)
  → 执行列表完成回调
  → 处理新 token
  → 构造 plane 并 PresentX
  → Signal(fence,value)
  → 发布批次完成状态

后续帧
  根据 fence/完成状态复用 allocator、命令段和资源
```

这个顺序说明为什么渲染后端替换不能只替换 Present：Draw 快路径、提交批次、token、资源地址和回收共同构成接口层。

## 10. 与老 RAGE 的语义对应

完整复核见 `LEGACY_ENGINE_COMPARISON.md`。当前架构可以解释为老 RAGE 的以下演进，而不是一套无关联的新渲染器：

- `grcGfxContext` 和 partial submit → 线程 GCL 池、命令段和异步提交环。
- `grcStateBlock::Flush` → requested/dirty/encoded 状态刷新。
- 独立 shader/state 对象 → `PipelineProgram` 和 PSO 缓存。
- placement buffer/texture → 地址池与 `CreatePlacedResourceX1`。
- XG `CopyIntoSubresource` → 创建/加载阶段的 tiled backing 初始化。
- COM SRV/UAV → descriptor heap 和 root binding。
- `WriteValueBottomOfPipe` / fence → Queue Signal 和跨帧延迟回收。
- `DXGIXPresentArray` → direct queue 多 plane `PresentX`。

这些对应只描述职责，不继承数值槽位或私有布局。尤其不能把老 fence 当新 frame token、把 CPU 侧 XG tiling 当 GPU copy，或把 `grcDevice::EndFrame` 当成 `Gfx_EndFrameAndFlush` 的逐句等价物。
