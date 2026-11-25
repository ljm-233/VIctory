# 高级渲染接口：StateObject、ExecuteIndirect 与 BVH

## 0. 证据范围

本文以 `game_scarlett_master.exe.i64` 为主证据，GDK 头文件只用于命名已经观察到的标准枚举和结构。地址前的 B 表示由二进制指令直接恢复；语义名称不代表原始源码符号。

对应伪代码：`pseudocode/09_low_level_calls.cpp`。

## 1. Raytracing StateObject

### 1.1 输入和输出

创建实现为 `RaytracingPipeline_CreateStateObject`，地址 `0x1424801B0`。其逻辑输入为：

```cpp
struct HitGroupShaderNames {
    const char* hitGroupExport;
    const char* anyHitShaderImport;
    const char* closestHitShaderImport;
}; // 24 字节
```

另有 ray-generation export、miss export 和 hit-group 数组。raygen/miss 是依据 identifier 存储位置和 association 范围确定的语义名。

实际访问的输出字段：

| 偏移 | 语义 |
|---:|---|
| `+0x118` | global root signature |
| `+0x120` | `ID3D12StateObject*` |
| `+0x128` | ray-generation local root signature |
| `+0x130` | miss/hit-group local root signature |
| `+0x138` | `ID3D12StateObjectProperties*` |
| `+0x140` | ray-generation shader identifier 指针 |
| `+0x148` | hit-group identifier 数组容量 |
| `+0x14C` | hit-group identifier 数量 |
| `+0x150` | hit-group identifier 指针数组 |
| `+0x158` | miss shader identifier 指针 |

这些 identifier 不是内联保存的 32 字节数据，而是 `GetShaderIdentifier` 返回的指针；StateObject 必须继续存活。

**共享对象边界（本轮新增）**：`0x141F77AD0` 分配的是 352 字节通用 ShaderProgram，`0x140F9F650` 根据 program 类型选择 native 编译路径。`+0x118` 的 global root signature 被 compute 和 DXR 共用；`0x14247FD30` 的 compute 路径也把 `+0x120` 用作 `ID3D12PipelineState*` 输出。因此上表是 **DXR 变体视图**，不能把其他 program 的 `+0x120` 一律解释为 StateObject。

### 1.2 N+9 个子对象

设 hit-group 数量为 N，最终描述为 `D3D12_STATE_OBJECT_TYPE_RAYTRACING_PIPELINE(3)`，子对象总数为 **N+9**：

| 顺序 | 类型 | 实际内容 |
|---:|---|---|
| 0 | `DXIL_LIBRARY(5)` | bytecode、去重后的 export 数组 |
| `1..N` | `HIT_GROUP(11)` | 每组 40 字节，`TRIANGLES(0)`，intersection 为空 |
| N+1 | `LOCAL_ROOT_SIGNATURE(2)` | ray-generation local root |
| N+2 | `SUBOBJECT_TO_EXPORTS_ASSOCIATION(7)` | 仅关联 ray-generation export |
| N+3 | `LOCAL_ROOT_SIGNATURE(2)` | miss/hit-group local root |
| N+4 | `SUBOBJECT_TO_EXPORTS_ASSOCIATION(7)` | miss + N 个 hit-group，共 N+1 个 export |
| N+5 | `RAYTRACING_SHADER_CONFIG(9)` | payload 16 字节、attributes 8 字节 |
| N+6 | `SUBOBJECT_TO_EXPORTS_ASSOCIATION(7)` | raygen + miss + N 个 hit-group，共 N+2 个 export |
| N+7 | `RAYTRACING_PIPELINE_CONFIG(10)` | 最大递归深度 1 |
| N+8 | `GLOBAL_ROOT_SIGNATURE(1)` | 输入对象的 global root signature |

DXIL export 列表的组成不同：raygen、miss，以及所有非空 any-hit/closest-hit shader 经过去重。hit-group 名称不加入该去重列表。每个 `D3D12_EXPORT_DESC` 都是 `ExportToRename=nullptr`、`Flags=NONE`。

association 直接引用其他子对象地址，因此构建代码必须预分配 N+9 个连续子对象，不能在保存引用后扩容。宽字符串和 export 数组必须存活到 CreateStateObject 和后续 identifier 查询全部结束。

### 1.3 创建和异步生命周期

- Device slot 101 / `0x328` 创建 StateObject。
- 随后 QueryInterface 得到独立的 `ID3D12StateObjectProperties`。
- 分别查询 raygen、miss 和 N 个 hit-group identifier；hit-group 数组按原顺序追加。
- `0x142480100` 可将创建投递到异步任务；返回 1 只表示任务已接收，不代表 object 已就绪。
- `ShaderPackage_GetBytecodeAndMaybeDecompress`（`0x14086F8E0`）提供 DXIL bytecode，必要时处理压缩数据；临时 shader 输入按内部所有权标志释放。

### 1.4 执行链审计：当前只证实创建，未证实 DispatchRays

本轮不再用孤立字节偏移猜调用，而是对整个 `.text` 做线性反汇编，只保留真实的间接 `call/jmp`。当前主 GCL 表的结果为：

| 方法 | 槽位 | 真实间接调用候选 | 语义核查结果 |
|---|---:|---:|---|
| `SetPipelineState1` | `0x3D0` | 42 | 均为其他游戏接口；未出现 StateObject 绑定证据 |
| `DispatchRays` | `0x3D8` | 28 | 均为其他游戏接口；未出现 SBT 描述与 XYZ 维度组合 |
| `DispatchRaysX` | `0x430` | 31 | 均为其他游戏接口；未出现 XS 扩展描述 |

候选中有大量返回整数/浮点数的配置、动画和数学虚函数，也有命中指令内部立即数的字节模式假阳性；只有完整指令边界和调用语义都符合才可接受。本轮未发现同时关联 StateObject、shader record 地址/stride/大小和 dispatch 维度的消费者。

设备初始化在 `0x1424785AD` 明确设置 `DisableDXR=TRUE`，即当前构建禁止运行库 DXR 加载和 scratch 分配；这与未找到 DispatchRays 相互印证。DXR 的创建代码仍存在，但不能把当前配置描述成启用光线追踪执行。

因此当前证据只支持：

- StateObject、Properties 和 identifier 的**创建合同**已恢复。
- 此 EXE 尚未提供可恢复的 SBT CPU/GPU 缓冲、record stride、DispatchRays 维度、Scratch/ExtraOptions 或异步创建后的执行等待合同；不能虚构这些布局。
- N+9 子对象没有 callable 子对象，因此该 pipeline 若以后执行，callable table 应为空；这不是已观察到 DispatchRays 描述为零。
- 异步 thunk `0x142480180` 只执行创建并释放任务。由于未找到执行消费者，也没有证据可进一步描述“SBT 如何等待异步对象”。

这不是断言游戏运行时绝不会执行 DXR。若动态 Hook 命中，应重新抓取 StateObject、GCL、SBT 和描述字节。对当前 title 的静态替换而言，DXR 创建已闭合，未命中执行链属于动态待验证项，而不是已知但漏恢复的活跃调用。

### 1.5 Shader blob 压缩格式

`ShaderPackage_GetBytecodeAndMaybeDecompress`（`0x14086F8E0`）在 package 不能直接提供连续字节码时读取 blob。若 blob 的低 16 位为 `0xDCE7`，`0x1451189F0` 按以下 12 字节小端头解码：

| 偏移 | 字段 | 含义 |
|---:|---|---|
| `+0x00` | `uint16 magic` | `0xDCE7` |
| `+0x02` | `uint8 codec` | 3 = Stored；13 = RawDeflate |
| `+0x03` | `uint8 reserved` | 原代码不检查 |
| `+0x04` | `uint32 totalBlobBytes` | 包含 12 字节头的总大小 |
| `+0x08` | `uint32 decodedBytes` | 输出容量及最终 bytecode 大小 |

- **Stored（3）**：`0x145118AF9` 从 `+12` 复制 `decodedBytes`。
- **RawDeflate（13）**：`0x145118AAA` 解码 `[+12, totalBlobBytes)` 到输出缓冲。`0x142157654` 明确实现 stored、fixed Huffman 和 dynamic Huffman block，因此是 raw DEFLATE，不是未命名的通用压缩，也没有 zlib/gzip 头解析。
- 输出由 `0x142477C60` 按 `decodedBytes` 从编译 scratch/临时节点池分配，随后受 ShaderProgram 临时输入所有权管理，不能作为长期独立对象保留。
- 原包装没有给其他 codec 建立有效输出；干净实现应显式拒绝，而不是返回未初始化数据。

对应可理解伪代码见 `07_pipeline_programs.cpp` 的 `ShaderBlobHeader` 和 `DecodeShaderBlob`。

## 2. ExecuteIndirect

### 2.1 五个 command signature

初始化证据为 `0x142479466..0x142479650`，Device `CreateCommandSignature` 使用 `+0x148`：

| 全局对象 | stride | 参数类型 |
|---|---:|---|
| `g_drawIndirectSignature` | 16 | `DRAW(0)` |
| `g_drawIndexedIndirectSignature` | 20 | `DRAW_INDEXED(1)` |
| `g_dispatchL2IndirectSignature` | 12 | `D3D12XBOX_INDIRECT_ARGUMENT_TYPE_DISPATCH_L2(105)` |
| `g_dispatchIndirectSignature` | 12 | `DISPATCH(2)` |
| `g_dispatchMeshIndirectSignature` | 12 | DispatchMesh，指令写入类型值 `0xA` |

本地旧 XS 头文件没有声明类型 10 的 DispatchMesh 枚举名，但初始化指令和后续使用直接证明该签名用途；不能将其误写成另一个 DispatchL2。

### 2.2 九个实际包装

全部使用 GCL slot 61 / `0x1E8`，`maxCommandCount=1`，count buffer 和 count size 都为零。

| 地址 | 语义 | 参数来源 |
|---|---|---|
| `0x14248D000` | Draw | 独立资源 |
| `0x14248D1B0` | Draw | 共享参数缓冲 |
| `0x14248D350` | DrawIndexed | 独立资源 |
| `0x14248D500` | DrawIndexed | 共享参数缓冲 |
| `0x14248CA90` | Dispatch | 独立资源 |
| `0x14248CBE0` | Dispatch | 共享参数缓冲，另有附加字节 |
| `0x140094360` | DispatchL2 | 独立资源 |
| `0x14248CD20` | DispatchL2 | 共享参数缓冲 |
| `0x144E33E10` | DispatchMesh | 独立资源 |

独立资源描述读取：

```text
+0x10  resourceFlags（任一位 0x2040 成立时使用附加大小）
+0x14  additionalArgumentBytes
+0x20  argument GPU VA
```

大小公式：

```cpp
argumentBytes = uint32(baseArgumentBytes)
    + ((resourceFlags & 0x2040) ? additionalArgumentBytes : 0); // 原 a5 为 32 位
```

共享缓冲同时具有 CPU 基址和 GPU VA：

```cpp
argumentBytes = argumentEndCpuAddress
    + additionalArgumentBytes
    - sharedCpuBaseAddress;
```

除 Dispatch 共享形式外，其余共享包装的附加字节为零。graphics 包装还检查当前上下文是否具备 indirect 执行资源；compute 包装没有同一条件。

### 2.3 桌面 D3D12 适配差异

XS 调用携带 argument/count buffer 的字节大小；桌面 `ExecuteIndirect` 使用 resource + aligned offset。兼容层应保留字节大小用于资源创建和验证，同时生成桌面 offset，不能机械照抄参数个数。

标准 Draw/DrawIndexed/Dispatch/DispatchMesh 可映射到桌面 command signature。DispatchL2 的附加直接参数位于 signature 而非每条 12 字节参数记录；初始化时 `mov qword ...,0x69` 同时把 `DispatchL2.Flags` 清零，即 `D3D12XBOX_DISPATCH_FLAG_NONE`。因此当前 EXE 的 DispatchL2 记录只含 XYZ 三个计数，可静态等价映射到桌面标准 Dispatch，没有额外 XS 调度标志需要翻译。

### 2.4 Direct DispatchMesh / DispatchMeshX 审计

全 `.text` 线性反汇编得到标准 `DispatchMesh=0x408` 的 337 个真实间接调用候选和私有 `DispatchMeshX=0x320` 的 34 个候选。按完整指令、参数和返回值语义筛选后，未发现由当前 graphics command list 发出的直接 mesh 调用；候选属于配置、动画、资源或其他游戏虚函数。当前实际确认的 mesh 执行入口仍是 `0x144E33E10` 的 ExecuteIndirect 包装。

AS/MS 的创建和绑定不因此缺失：`key.UsesMeshShaderPipeline()` 将 VS/HS 子对象类型改成 AS/MS，随后仍走 `CompileGraphicsProgram`、PSO cache、compile ready 和普通 `SetPipelineState` 刷新。桌面间接参数的 12 字节记录就是三个 `uint32 ThreadGroupCountX/Y/Z`；不要把私有 `D3D12XBOX_DISPATCH_MESH_X_ARGUMENTS` 或间接资源描述误当成 direct 原型。

因此 direct DispatchMesh 是动态待验证项，不是已知但漏恢复的活跃合同。若以后命中，应补录 XYZ 参数、对应 PSO 和 barrier；当前不虚构 direct wrapper。

## 3. BVH build-info 与 80 字节批量描述

`BvhBuildInfo` 已确认的前 `0x48` 字节：

```text
+0x00  D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO（24 字节）
+0x18  resourceFlags
+0x1C  compactedSizeSourceOffset
+0x20  compactedSizeSource / ID3D12Resource*
+0x28  D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS（32 字节）
```

批量项为 80 字节：

```text
+0x00  D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC（48 字节）
+0x30  NumPostbuildInfoDescs
+0x38  pPostbuildInfoDescs
+0x40  ExtraOptions.SizeInBytes
+0x48  ExtraOptions.pXdxrStreamDesc
```

当前填充路径中 ExtraOptions 保持零。

### 3.1 三条 PrebuildInfo 入口不能合并

- `0x140075480` 在 Morton 位设置时构造 **TOP_LEVEL instance inputs**，否则构造 BOTTOM_LEVEL triangles/AABBs。已恢复 instance filter、instance flags、transform 复制、地址 alias、stride、compaction 和 update scratch；非 update 时清零 `UpdateScratchDataSizeInBytes`。
- `0x144EE2A00` 从 48 字节几何记录构造 56 字节 triangles/AABBs，不改变 Morton，分别输出 result/scratch；triangles 的 transform 非空时保留原 `UINT64_MAX` 哨兵路径。
- `0x1456AC100` 始终临时启用 64 位 Morton，查询后恢复 false，没有可选 bool 参数。

对应伪代码已拆成三个入口，packed 几何和 instance 转换均已展开。

## 4. 两条 BVH 构建路径

实现：`Bvh_BuildBatchedWithCompactionAnd64BitPaths`（`0x140FE4E50`）；`0x142490110` 是另一对应实现。

### 4.1 compaction/update 路径

每项 update 标志会影响三个字段：

- scratch 大小：update 使用 `UpdateScratchDataSizeInBytes`，否则使用 `ScratchDataSizeInBytes`，均按 256 字节对齐。
- `Inputs.Flags`：update 时加入 `PERFORM_UPDATE`。
- `SourceAccelerationStructureData`：update 时等于该项原目的地址，否则为零。

只有 `ALLOW_COMPACTION && !update` 的项生成一个 `COMPACTED_SIZE` postbuild 查询。查询目的地址为全局 compacted-size buffer 的 GPU VA 加 `queryIndex * stride`。

批次同时受两个条件限制：

1. 最多 16 项。
2. 下一项不能放入剩余 scratch 时先提交当前批次，并在填充下一项之前回绕到 scratch 基址。

scratch 回绕时记录 UAV barrier；并不是每 16 项机械记录一次 barrier。

全部构建后，压缩查询结果从全局 compacted-size buffer 复制到各 build-info 的 compacted-size resource。RAGE 状态映射表确认 `5→24` 是 `UNORDERED_ACCESS → COPY_SOURCE`，复制后恢复 `24→5`：

- 状态 5：sync=`ALL_SHADING(0x1000)`、access=`UNORDERED_ACCESS(0x10)`、layout=`UNORDERED_ACCESS(3)`。
- 状态 24：sync=`COPY(0x200)`、access=`COPY_SOURCE(0x800)`、layout=`COPY_SOURCE(7)`。

全局 buffer 是复制源，build-info 资源才是复制目的。参数为：

```cpp
list.CopyBufferRegion(
    info.compactedSizeSource,
    (info.resourceFlags & 0x2040) ? info.compactedSizeSourceOffset : 0,
    compactedSizeBuffer->resource,
    queryIndex * stride,
    stride);
```

数值名称与 [DirectX Enhanced Barriers 规范](https://microsoft.github.io/DirectX-Specs/d3d/D3D12EnhancedBarriers.html)交叉核对，状态标签本身来自 EXE 的三张映射函数。

### 4.2 64 位 Morton 路径

- 构建前 `Set64BitMortonCodes(true)`，完成后恢复 false。
- 只按 16 项分批，不执行 compaction/update 路径的 scratch 容量回绕和 compacted-size 查询。
- 每项始终使用普通 `ScratchDataSizeInBytes`；`performUpdates` 不参与此路径的 scratch 选择，也不加入 `PERFORM_UPDATE`。
- 最终记录一个 UAV barrier。

## 5. COMPACT copy 与 rehydrate

### 5.1 COMPACT copy

`0x140BD96A0` 对每对 destination/source 调用：

```cpp
CopyRaytracingAccelerationStructure(
    list, destination, source,
    D3D12_RAYTRACING_ACCELERATION_STRUCTURE_COPY_MODE_COMPACT);
```

每项 copy 后仍执行条件性图形工作冲刷；整个序列结束后只记录一个 UAV barrier，不是每项一个 UAV barrier。

### 5.2 Rehydrate

`0x140EA6C60` 对每个非空序列化源和非零目的地址：

1. 必要时转换 CPU alias。
2. 计算序列化数据偏移。
3. 检查 `serialized[dataOffset + 3] & 1`。
4. 填充目的地址、build-info Inputs、Source 和 originalInstanceData。
5. 满 16 项提交一次，最后提交剩余项，并在确有记录时补一个 UAV barrier。

偏移来自标准 `D3D12_SERIALIZED_RAYTRACING_ACCELERATION_STRUCTURE_HEADER`，其大小恰为 56 字节：

```cpp
const auto& header = *reinterpret_cast<
    const D3D12_SERIALIZED_RAYTRACING_ACCELERATION_STRUCTURE_HEADER*>(serialized);
dataOffset = header.SerializedSizeInBytesIncludingHeader
    < header.DeserializedSizeInBytes
        ? sizeof(header) // 56，大小关系异常时退回标准头末尾
        : header.SerializedSizeInBytesIncludingHeader
            - header.DeserializedSizeInBytes;
```

`+0x20` 是 `SerializedSizeInBytesIncludingHeader`，`+0x28` 是 `DeserializedSizeInBytes`，`+0x30` 是此路径未读取的 `NumBottomLevelAccelerationStructurePointersAfterHeader`。两个输出地址不能合并：`SourceAccelerationStructureData = 原始 serializedSource + dataOffset`，而 `originalInstanceData = 转换后的 CPU alias + dataOffset`。原始指令分别使用 `v14` 和 `v16`，不能把 CPU alias 同时写入 Source。

## 6. 对桌面接口改造的影响

- StateObject 应按本文逻辑结构重建标准桌面 DXR 描述，不复制 XS 私有内存块。
- 九个 ExecuteIndirect 包装应分别映射，不能用统一 DrawIndexed 或统一 Dispatch 代替。
- 桌面 BVH 构建可使用标准 build desc，但 compaction query、scratch 生命周期、UAV 可见性和复制回读必须保留。
- Rehydrate 是 XS 专用路径；桌面需要基于序列化数据语义实现等价恢复，不能只创建一个同名空函数。
- 原 XS barrier 和内部状态值用于理解可见性，不直接作为桌面 D3D12 数值传入。
