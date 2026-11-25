# 渲染快速路径专项分析

本文分析当前 EXE 实际存在的快速路径，不再只讨论“关闭快路径后的替换”。权威证据是当前 IDB；参考项目只用于确认对象模型。

## 1. 公共机制

GCL 内联编码依赖：

- `+0x18`：当前 put/write cursor。
- `+0x28`：Draw 使用的软限制。
- `+0x30`：其他内联包使用的分配限制。
- `+0x140`：`hasEncodedState`，表示存在需要合并到后续包的状态。
- `+0x141`：PSO `stateAndFlags` bit20 对应的特殊状态。
- `+0x142`：是否已经写过 Draw。
- `+0x143`：其他包变体状态。
- `+0x144`：root opcode 表。
- `+0x544..+0x563`：32 字节 PSO shadow。
- `+0x548`：命令段标志，低两位影响 root opcode 区域选择。

多数内联分支只检查 `put < limit`，不显式验证整个包是否越界；运行库命令段已经为包预留空间。逐地址复核后，伪代码的 `HasSpace(bytes)` 也只表达这一原始比较，`bytes` 仅保留调用点的包大小信息，不再使用更保守的 `put+bytes<=limit` 替代原行为。

如果 put/limit 不满足条件，各类路径分别调用 `IASetPrimitiveTopology`、`SetPipelineState`、`IASetIndexBuffer`、`IASetVertexBuffers`、`OMSetStencilRef`、root setter 或 `DrawIndexedInstanced`。因此“原生包生成”和“XS 方法调用”是两种等价后端路径，不是两套渲染语义。

## 2. 九类硬件内联路径

### 2.1 Topology — 8 字节

证据：`0x14003258A..0x1400325BA`。

```cpp
output[0] = 0xC000B600;
output[1] = TranslateTopology(requested); // 6/19 映射为 4，其余保持
put += 8;
```

慢路径：`IASetPrimitiveTopology(translated)`。只在 requested 与 encoded 不同的时候写入。

### 2.2 PSO — 28 字节包 + 32 字节 shadow

证据：`0x140032786..0x1400327C5`。

原生 PSO `+0x10` 开始的 32 字节为：

| 原生偏移 | 大小 | 用途 |
|---:|---:|---|
| `+0x10` | 16 | `commandPrefix`，同时进入 shadow 和命令包 |
| `+0x20` | 8 | `stateAndFlags`，bit20 写入 GCL `+0x141` |
| `+0x28` | 4 | `commandTail`，命令包最后 4 字节 |
| `+0x2C` | 4 | `shadowOnly`，不进入命令包 |

无论快慢路径都先把 32 字节复制到 GCL `+0x544`。快路径再把前 28 字节写入命令流，并设置：

```cpp
hasEncodedState = 1;
hasSpecialPipelineState = (stateAndFlags & 0x100000) != 0;
put += 28;
```

慢路径：`SetPipelineState(nativePso)`。

**边界：**这 32 字节由 XS PSO 创建/反序列化产生，当前 EXE 不包含从高层 PSO key 生成它们的算法。使用原生快路径时必须保留真实 XS 数据，或继续逆向运行库 PSO 生成器；不能根据 stream 子对象直接编造这 32 字节。

### 2.3 Index Buffer — 12 字节

证据：`0x14003282A..0x1400328B8`。

```cpp
output[0] = 0x4001B700 | format;
write64(output + 4, bufferLocation);
put += 12;
```

format 为 4 时选择 42，否则为 57。空 resource 不走普通包写入，空间不足时调用 `IASetIndexBuffer(nullptr)`。

### 2.4 Vertex Buffers — `4 + 8*N` 字节

证据：`0x1400328C2..0x140032960`。

```cpp
output[0] = (count << 17) + 0x3FFFC600;
for each view:
    write64(output, view.BufferLocation + (uint64_t(view.StrideInBytes) << 48));
```

`SizeInBytes` 不进入内联包。每项只包含低 48 位地址和高 16 位 stride。写完后 `hasEncodedState=1`；慢路径为 `IASetVertexBuffers(0,count,views)`。

### 2.5 Stencil Ref — 8 字节

证据：`0x14003296A..0x14003299B`。

```cpp
output[0] = 0xC000B800;
output[1] = stencilRef;
put += 8;
```

慢路径：`OMSetStencilRef(stencilRef)`。

### 2.6 Root Descriptor Table — 8 字节

证据：`0x140032175..0x1400322E0`。

```cpp
output[0] = SelectRootDescriptorTableOpcode(rootOpcode[rootIndex]);
output[1] = uint32_t(gpuDescriptorHandle); // 只写低 32 位
put += 8;
```

如果基值 `<0x40000000`，只置 `hasEncodedState=1`。否则当 `(base&0xFF00)<0xE100` 且段标志低两位非零时，opcode 加 `0x400`。

### 2.7 Root SRV/UAV — 12 字节

证据：`0x1400322EE..0x140032422`。

```cpp
output[0] = SelectRootGpuAddressOpcode(rootOpcode[rootIndex]);
write64_unaligned(output + 4, gpuVirtualAddress);
put += 12;
```

SRV/UAV 写完整 64 位地址，与 descriptor table 的低 32 位 handle 不是同一种包。候选修正为 `+0x100` 或 `+0x500`，由 opcode 高位区域和段标志共同决定。

### 2.8 Root Constants — `4 + 4*N` 字节

证据：`0x14003243A..0x140032514`。

```cpp
base = rootOpcode[rootIndex];
if (base < 0x40000000) {
    hasEncodedState = 1;
} else {
    if (count == 2) base += 0x100;
    else if (count == 4) base += 0x200;
    else if (count != 1) base += 0x300;
    if ((base&0xFF00)<0xE100 && (encodingFlags&3) != 0)
        base += 0x400;
}
output[0] = base + (count<<16) - 0x10000;
copy_n(constants, count, output + 1);
```

慢路径：`SetGraphicsRoot32BitConstants(rootIndex,count,values,0)`。

### 2.9 DrawIndexed — 24 字节

证据：`0x14248CE00`。

```cpp
struct DrawIndexedPacket {
    uint32_t opcode;
    uint32_t instanceCount;
    uint32_t startIndex;
    uint32_t indexCount;
    uint32_t baseVertex;
    uint32_t startInstance; // 当前路径固定为 0
};
```

opcode 选择：

```cpp
firstUnadorned = drawPacketVariant == 0 &&
                 hasEncodedState == 0 &&
                 otherPacketVariant == 0;
opcode = firstUnadorned ? 0x4004E700 : 0x4004C200;
```

快路径写入后：

```cpp
hasEncodedState = 0; // 清 +0x140，不是 +0x141
drawPacketVariant = 1;
put += 24;
```

慢路径：`DrawIndexedInstanced(indexCount,instanceCount,startIndex,baseVertex,0)`。`currentProgram==nullptr` 时整个 Draw 被跳过。

### 2.10 ExecuteIndirect — 直接 XS 调用

`0x14248CA90/0x14248CBE0/0x14248CD20/0x14248D000/0x14248D1B0/0x14248D350/0x14248D500/0x140094360/0x144E33E10` 共 9 个包装，调用 GCL slot 61 / `0x1E8`：

```cpp
list.ExecuteIndirect(
    signature, maxCommandCount,
    argumentBuffer, argumentBufferSize,
    countBuffer, countBufferSize);
```

初始化在 `0x1424794AC..0x142479650` 创建 Draw、DrawIndexed、DispatchL2、Dispatch 和 DispatchMesh 五类 command signature。9 个包装全部 `maxCommandCount=1`、count buffer 为空；间接绘制不复用 DrawIndexed 的 24 字节内联包，必须单独纳入后端覆盖。

### 2.11 Compute root/PSO 变体

`0x14248C2A0` 和 `0x14248C6B0` 是 compute 侧状态刷新变体，也会复制 PSO 的 32 字节 shadow、使用 28 字节 PSO 快路径，并处理 compute root descriptor。它们最终同样可进入 ExecuteIndirect，说明仅分析 graphics root 并不足以覆盖计算渲染。

## 3. CPU 和资源管理快速路径

这些路径不产生 PM4 包，但同样影响后端性能与正确性：

| 快速路径 | 已确认机制 | 替换注意点 |
|---|---|---|
| Descriptor bump allocator | 原子对齐 CAS 推进 cursor | 需要保持 increment 和对齐，不能把它当普通主机指针分配 |
| Descriptor free-list | packed head 低 32 位索引，CAS 读取 `nextFree` | 返回节点地址作为 CPU descriptor handle，并保留 ABA 高位 |
| Direct GCL pool | packed free-list head + ResetX | 列表节点和 allocator 必须配对复用 |
| Compute GCL pool | 锁和 round-robin 上下文选择 | 不等于 direct 的 lock-free 算法 |
| PSO 发布快照 | 读侧共享锁命中已发布表，未命中再进入写锁 | 编译中的 program 不能直接当 ready PSO 使用 |
| 派生 PSO 缓存 | hash/base PSO 匹配，失败才创建派生描述 | 派生顺序和 332 字节 ABI 必须一致 |
| Barrier 聚合 | 组容量 257，每类资源容量 1025，达到上限先 flush | `count` 是组数，不是资源转换数 |
| 批量提交 | 最多 256 个 GCL，Present/token 为排序边界 | 回调在 CPU 提交后执行，不代表 GPU 完成 |
| 线程 scratch | 深度计数复用 200 KiB 临时区 | 异步任务不能继续持有 scratch 中的 stream 或字节码 |

## 4. 结论与剩余边界

九类硬件内联路径的**包大小、写入公式、状态更新和慢速回退均已定位**。软件 CBV 也有独立 16 字节公式，见 `04_descriptors_copy.cpp`。

唯一不能仅由当前 EXE 完整生成的是原生 PSO 的 32 字节 `NativePsoInlineData` 内容：布局和使用方式已恢复，但生成算法在 XS 运行库而非游戏 EXE。如果替换后端需要原生 PSO 快路径，必须：

1. 保留真实 XS PSO 创建/反序列化结果；或
2. 继续逆向 XS PSO 生成器；或
3. 初版强制走 `SetPipelineState`，将此作为后续性能工作，而不是阻塞正确替换。

具体降级步骤、ResetX 后的重复处理和 CBV 单独降级见 `FORCED_SLOW_PATH.md`。
