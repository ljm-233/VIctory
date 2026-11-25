# 渲染接口端到端工作原理

## 1. 启动顺序

```text
Gfx_InterfaceInit
  → 初始化帧状态、deferred 容量、队列/池/锁和 25 项帧接口
Platform_GraphicsBoot
  → GraphicsVirtualMemory.ConfigureOnce（必须先于图形分配）
  → 创建窗口和 Device11
  → 创建 direct queue + 四个 compute queue
  → 创建 queue contexts、列表槽和 direct/compute 池
  → 为初始 GCL 写入 pipe/cache 前导并 Close
  → 创建 descriptor heap、root signature、PSO/资源系统
  → 创建默认 SRV/UAV/CBV 和帧视图
```

### 初始 GCL 前导

direct：

```text
SyncBottomOfPipeX(CS|VS|PS partial flush, no control)
SyncTopOfPipeX(K/V/GL1/GL2 invalidate + GL2 writeback + PFP_SYNC_ME,
               PFP engine + GCR reverse)
Close
```

compute：

```text
SyncBottomOfPipeX(CS partial flush, no control)
SyncTopOfPipeX(K/V/GL1/GL2 invalidate + GL2 writeback,
               GCR reverse)
Close
```

这些操作只录制同步命令，不是 CPU 等待，也不证明 GPU 已空闲。

## 2. 资源创建

### committed

```text
RageResource 描述
  → heap properties / heap flags / DESC1 / initial layout / clear value
  → CreateCommittedResource3
  → ID3D12Resource
```

存储由运行库按 heap 属性负责，引擎不再单独提供 GPU VA。

### placed

```text
RageResource 描述
  → 按 memoryType/subtype 分配地址，或复用调用者地址
  → 可选：XGTextureAddressComputer 把线性像素写入 tiled backing
  → 转换 placement 地址别名
  → CreatePlacedResourceX1(address, DESC1, layout, clear, IID, out)
  → 失败时只释放本路径拥有的地址
```

backing-only 分支故意允许 `resource==nullptr`，后续再实例化 D3D 对象，不能把它当异常。

### buffer 与 CBV

```text
elementCount * stride
  → 地址分配和可选初始 memcpy
  → 按格式/内存类型决定是否创建 ID3D12Resource
  → 保存 GPU/CPU 地址
  → 需要 CBV 时：
       原生模式：CreateConstantBufferView
       软件模式：直接写两个硬件描述字
```

## 3. 每线程录制

```text
游戏/材质请求
  → 修改 requested state 和 dirty bits
  → DrawIndexed / Dispatch / Copy
  → 懒获取线程命令上下文和 GCL
  → 合并 requested/deferred/encoded dirty 状态
  → 刷新 PSO、IA、RT/DS、viewport/scissor、root bindings
  → FlushPendingBarriers（按 D3D12_BARRIER_GROUP 分组）
  → 有空间：直接写命令包并推进 cursor
     无空间：调用对应 XS 方法
```

### 状态含义

- `requested`：调用者希望下一命令使用的状态。
- `dirty`：requested 与 encoded 的差异以及显式失效位。
- `encoded`：已经写入当前命令段的状态。
- `completed`：GPU 已执行的状态，只能由 fence/同步查询证明，不能用 dirty=0 代替。

### root 描述符

```text
shader binding layout
  → 需要重建时分配 CPU descriptor staging
  → 按需生成/更新 CBV、SRV、UAV 描述符
  → 空绑定回退默认描述符
  → 32 字节记录复制到 staging
  → CPU staging 地址换算 GPU descriptor handle
  → 写 root table 包（8 字节）
```

root SRV/UAV 地址包为 12 字节，root constants 为 opcode 加 N 个 32 位值。

## 4. PSO 创建与复用

```text
GraphicsProgramKey
  → hash
  → 无锁发布表查找
  → 加锁写入表二次查找
  → 未命中：创建逻辑 program 并插入
  → 异步任务或当前线程 CompileGraphicsProgram
       → derived PSO cache 查找
       → 未命中：688 字节 stream + CreatePipelineState
  → ready 后供 Draw 使用
```

compute：

```text
shader hash
  → serialized cache 查找 + DeserializeComputePipelineStateX
  → 未命中：CreateComputePipelineStateX
```

编译 scratch 只在创建调用作用域内有效，PSO 对象与逻辑 program 的生命周期独立于该临时区。

## 5. 提交与 Present

```text
渲染线程
  → Close / 生成 SubmitCommandList 记录
  → FrameToken / Callback / Present 记录进入 96 批提交环

提交消费者
  → 收集 GCL、完成 handler 和上下文
  → 达到记录/帧/token/容量边界时批量 ExecuteCommandLists2X
  → 执行 CPU 完成 handler
  → FrameToken：
       显式设置，或以 PROCESS_TOKEN_ON_TIMEOUT 等待并生成 token 后记录
  → Present：
       24 字节内部 plane 展开为 0x48 XS plane
       PresentX
       清 current token
  → Signal(completionFence,value)
  → 写 CPU completion value 并释放批配额
```

批量 Execute 使用 `NO_SWITCH_BUFFER | CHAIN_SEGMENTS`；单列表包装使用 `NONE`。回调发生在消费者提交之后，不代表 GPU 完成；资源复用必须依赖 fence/完成状态。

## 6. RAGE 帧控制

```text
GameFrame_Update
  → 渲染和构建帧数据
  → IsFramePending？
       是：EndFrameAndFlush
            → 推进旧帧引用
            → 检查 scope/end-request 和帧锁
            → 需要时销毁旧显示资源
            → pending 时执行全部 deferred 闭包
            → 清 pending/end-request
            → 需要时重建显示资源
            → 清 flush-in-progress
       否：继续后续 job
```

`Gfx_FlushDeferredResources` 注册 48 个闭包，不是立即执行 48 个 pass。闭包执行后仍保留，后续帧可再次 pending。

`Pass_Tonemapping_Wrapper` 展示典型 scope：BeginFrameScope → 执行 pass → EndFrameScope。scope depth 影响普通 EndFrameAndFlush 能否进入，显式 end-request 可强制进入。

## 7. 资源销毁与重建

```text
presentationResourcesReady=true
  → 遍历每个 72 字节 PresentationViewState
  → 释放旧 texture/buffer views、SRV/UAV、RT/DS
  → 迁移 tagged resource-state 链
  → 释放临时存储
  → ready=false

后续重建
  → 收集每 view 的 pending updates
  → 重建资源
  → 重建 SRV/UAV 描述符
  → 重建 RTV/DSV
  → ready=true
```

这条路径与窗口尺寸变化、显示模式变化和资源失效相关，不应与每帧 Present 混为一谈。

## 8. 关闭原则

正确顺序是：停止生产 → 消费剩余提交 → fence 完成 → 释放延迟链 → 释放列表池和 allocator → 释放队列/资源/descriptor → 释放 Device11。仅释放 COM/GraphicsUnknown 指针而不等待 GPU，会使命令段、placed backing 和回调上下文悬垂。
