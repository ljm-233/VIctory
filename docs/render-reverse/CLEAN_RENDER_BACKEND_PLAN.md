# Scarlett 干净桌面 D3D12 渲染引擎实现计划

## 0. 方案决定

最终方案是：**不提取、不移植 GTA5 渲染代码，不以 D3D11 作为过渡后端，直接按当前 EXE 的渲染语义重新实现干净的桌面 D3D12 渲染引擎。**

`game_scarlett_master.exe.i64` 仍是接口、布局、状态、执行顺序和生命周期的唯一权威。GTA5 老源码、GDK 文档、头文件及其他项目只用于理解或交叉核对，不进入新实现的代码基线，也不能覆盖 EXE 证据。

目标是完整替换当前渲染后端，而不是仅 Hook 几个 Draw。普通图形、compute、copy、ExecuteIndirect、Query、AS/MS、StateObject/BVH、多 queue、上传回读和 Present 都必须按当前实际合同实现。中间诊断可以临时关闭高级 pass，但不能把永久裁功能写成完成。

配套文档：

- `D3D12_IMPLEMENTATION_MANIFEST.md`：新引擎组件、职责、实施顺序和验收物。
- `INTERFACE_MIGRATION_MAP.md`：当前 EXE 语义到干净桌面 D3D12 接口的逐项映射。

## 1. 实施路线

```text
完成当前渲染语义和证据边界
→ 定义 API 无关的公共描述、强类型句柄和生命周期合同
→ 用原 XS 后端记录可重复场景与资源/命令基线
→ 实现干净的桌面 D3D12 设备、资源、program、命令和帧核心
→ 接入当前普通 pass，原 XS 编码器退出实际渲染
→ 实现高级接口和多 queue
→ 逐 pass 验证资源内容、命令语义、同步和画面
→ 全功能正确后再做内存与提交性能优化
```

不设置 D3D11 基线阶段。对照基线来自原 Scarlett 后端的帧日志、readback、截图和 PIX capture，而不是另一套老引擎实现。

## 2. 新架构

```text
当前游戏高层 pass / 材质 / 场景请求
                  │
                  ▼
ScarlettRenderInterface（新公共语义层）
  ResourceDesc / ProgramDesc / BindingDesc
  CommandEncoder / SubmissionDesc / PresentDesc
  FencePoint / DeferredRelease / BackendCapabilities
                  │
                  ▼
NativeD3D12RenderEngine（唯一新后端）
  Device+Queue       Resource+Heap       Shader+PSO
  Descriptor+State   Command+Submission  Frame+Present
  Indirect+Query     Mesh+DXR+BVH        Upload+Readback
                  │
                  ▼
公开桌面 D3D12 / DXGI / DXC 或明确的 shader 编译链
```

公共接口表达当前游戏需要的逻辑语义，不暴露 Device11/GCL 槽位、XS packet、统一内存地址或原生 COM 布局。新后端内部只使用公开桌面 D3D12 模型，不模拟 XS vtable，也不恢复 Scarlett 私有硬件命令流。

## 3. 公共合同

以下是新接口设计，不宣称原 EXE 存在同名类或相同布局。

```cpp
enum class CommandDomain { Direct, Compute, Copy };

struct ResourceHandle { uint32_t index, generation; };
struct ProgramHandle { uint32_t index, generation; };
struct FencePoint { uint32_t queue; uint64_t value; };

class IRenderEngine {
public:
    virtual ResourceHandle CreateResource(const ResourceDesc&) = 0;
    virtual ProgramHandle CreateProgram(const ProgramDesc&) = 0;
    virtual void DestroyDeferred(ResourceHandle) = 0;
    virtual ICommandEncoder& GetEncoder(CommandDomain, ThreadContext&) = 0;
    virtual FencePoint Submit(const SubmissionDesc&) = 0;
    virtual bool IsComplete(FencePoint) const = 0;
    virtual void Present(const PresentDesc&) = 0;
};

class ICommandEncoder {
public:
    virtual void SetPipeline(ProgramHandle) = 0;
    virtual void SetBindings(const BindingSet&) = 0;
    virtual void Draw(const DrawArguments&) = 0;
    virtual void Dispatch(const DispatchArguments&) = 0;
    virtual void Copy(const CopyRequest&) = 0;
    virtual void Barrier(const BarrierSet&) = 0;
    virtual void ExecuteIndirect(const IndirectRequest&) = 0;
    virtual void DispatchMesh(const DispatchMeshArguments&) = 0;
    virtual void DispatchRays(const DispatchRaysRequest&) = 0;
};
```

实际代码应拆成多个组件，不形成单一巨型 Device。高级接口可以位于专用 encoder/device 组件中，但共享同一资源、同步和回收体系。

### 3.1 资源与内存

- 公共描述保留格式、尺寸、mip/slice、用途、CPU 访问、初始数据和 backing 所有权。
- D3D12 后端明确区分 default、upload、readback 和 custom heap；不能把所有资源放进 upload heap 来模拟统一内存。
- backing-only、无 `ID3D12Resource` 的描述必须保留其语义，不能强制创建假资源。
- 上传使用 ring/arena 与 GPU copy；readback 必须等 fence 完成后映射。
- 水面物理按 `WATER_SIMULATION_READBACK.md` 单独实现三槽 readback：Compute → COPY_SOURCE → READBACK → 每槽 fence → CPU 旧结果查询。不得在每次物理查询时等待整个 queue，也不得把 OceanWaveSim 的全部输出无差别下载。
- 回收由 GPU fence 驱动，CPU 引用归零不等于可以释放。
- placed heap、aliasing 和页映射只有在语义需要或实测有益时引入，初版优先 committed resource 的正确性。

### 3.2 Shader、program 和绑定

- `ProgramDesc` 表达 shader 阶段、入口、输入输出、绑定布局、固定状态和 topology。
- 建立可编译源码或明确中间表示到桌面 DXIL 的工作流；XS blob 不能默认直接在桌面驱动使用。
- graphics、compute、AS/MS、DXR 分别建立编译和缓存键，异步任务必须传播失败并公开 ready 状态。
- graphics/compute PSO、root signature、descriptor 和动态状态分别缓存；不能只按 shader hash 复用整个 PSO。
- 空资源使用类型正确的默认 descriptor，不能用空指针绑定冒充完成。

### 3.3 命令、queue 和同步

- 每线程命令上下文独立管理 allocator/list， fence 完成前不得 Reset 或复用。
- direct/compute/copy domain 使用强类型标识；queue index、pool index、frame serial 和 fence value 不混用。
- resource state tracker 生成真实 barrier，保持当前 flush、可见性和跨 queue 顺序。
- CPU 完成回调和 GPU fence 分开；提交返回不代表 GPU 完成。
- Present 保留帧提交关系、视图、矩形、色彩和多 plane 请求，桌面通过 swapchain 或显式合成实现。

## 4. 分阶段实施

### A. 冻结语义与建立原后端基线

1. 完成 DXR、direct DispatchMesh、Query 和 shader/package/program 的证据审计。
2. 固定可重复场景，记录资源描述、program key、绑定、命令、提交、Present 和回收顺序。
3. 对关键 render target、compute 输出和 readback 保存内容基线。
4. 标记静态未证实项和动态未命中项，不用猜测补齐。

**门槛**：每个普通 pass 和高级 pass 都能对应到输入、输出、命令与生命周期；未决项有明确证据边界。

### B. 新 D3D12 核心骨架

1. Device、adapter、swapchain、direct queue、allocator/list 和 fence。
2. 强类型句柄、资源/程序注册表、deferred release 和错误传播。
3. committed resource、upload/readback、descriptor heap 和基本 state tracker。
4. 最小 graphics/compute PSO、Draw/Dispatch/Copy、Present 和 resize。

**门槛**：独立测试完成清屏、纹理、基础 Draw、compute、copy、Present 和退出；debug layer 无新增错误，重复 resize 和最小化恢复稳定。

### C. 接入当前普通渲染

1. 将当前 pass 收敛到新公共接口，原 XS 编码器不再参与这些 pass。
2. 完成格式、输入布局、program/binding、动态常量、纹理上传和 render target 映射。
3. 保持 deferred 回调顺序、帧作用域、资源视图生命周期和 Present 记录持久性。
4. 用 A 的日志和 readback 逐项对照，不只比较最终截图。

**门槛**：普通图形、compute、copy、上传、回读和 Present 在真实场景工作；无静默空 Draw、缺失 barrier、在途上传覆盖或提前回收。

### D. 高级功能

1. 五类 command signature 与九个 ExecuteIndirect 包装。
2. QueryHeap、Begin/End/Resolve/readback；若静态只创建未执行，先用动态命中确认实际合同。
3. AS/MS PSO 和 direct/indirect DispatchMesh。
4. StateObject、Properties、SBT、SetPipelineState1、DispatchRays 和异步就绪。
5. BVH build/update/compaction/copy/rehydrate/prebuild 和 UAV 可见性。
6. 1 direct + 4 compute queue、跨 queue fence、资源同步和列表池。

**门槛**：每个当前实际使用的高级功能都有真实执行场景；不存在空成功返回或永久屏蔽。XS 私有描述按逻辑语义重建，不复制未知字节。

### E. 全量替换与性能

1. 原 XS 后端退出全部实际渲染，不再依赖其私有 ABI。
2. 长时间运行、设备移除、resize、全屏切换和内存压力测试。
3. 优化多帧资源 renaming、上传 arena、descriptor/PSO cache、命令列表复用和多 queue。
4. 只有测量证明需要时才引入 placed heap、aliasing 或其他复杂内存路径。

**门槛**：全功能、内容正确和稳定性先通过，再接受性能优化；每项优化都能关闭并回退到普通 D3D12 路径。

## 5. 不采用的做法

- 不复制 GTA5 `grcDevice`、D3D11 stateblock、effect 或资源实现。
- 不维护 D3D11/D3D12 两套高层渲染逻辑。
- 不用 D3D11On12 代替原生 D3D12 验收。
- 不在公共层暴露 D3D11 COM、XS vtable 或 GCL 写指针。
- 不用 capabilities 永久删除当前功能；它只用于硬件能力检查和中间诊断。
- 不把“设备创建成功”“没有崩溃”或“截图大致相似”当完整替换证明。

## 6. 最终完成定义

1. 新实现不包含提取的 GTA5 渲染代码，公共语义与桌面后端职责清晰。
2. 当前游戏所有 pass 只走新接口，原 XS 后端不再参与实际渲染。
3. 普通图形、compute、copy、上传、readback、Present 和跨帧回收通过真实场景。
4. 当前实际使用的 Indirect、Query、AS/MS、StateObject/BVH 和多 queue 均在桌面 D3D12 中实现并执行。
5. shader/program 来源、编译、缓存和失败传播闭合，不依赖无法使用的 XS blob。
6. 资源内容、命令语义、画面和同步达到原后端基线，长时间运行无死锁、错帧、提前释放或明显泄漏。
7. 性能优化发生在全功能正确之后，并保留普通路径回退。

本文是实施计划，不表示新引擎已经编译、接入或通过实机验证。当前交付物仍主要是静态逆向、伪代码和接口合同。
