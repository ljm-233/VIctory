# 水面模拟：GPU 计算与 CPU 物理采样

## 结论与证据边界

当前 EXE 同时存在两条不同的水面 GPU 路径：

1. `water_compressed_simulation_rendertarget` / `water_heightpass`：包含 Compute 调度、模拟结果记录和三槽 CPU 采样。
2. `OceanWaveSim`：也是 Compute 调度，但目前只确认资源创建、UAV/SRV 绑定和后续 GPU 使用，未确认 CPU 读取。不能把它的输出全部纳入物理 readback。

可以确认 CPU 会通过资源的 **CPU alias** 直接采样 `R32G32_FLOAT` 数据，并使用三槽索引避开正在生成的帧。当前静态分析尚未逐一证明“某次 Compute dispatch 写入的原生资源”与“某次 CPU 查询读取的 alias”一定是同一底层资源；GPU pass 和 CPU 查询的对象/记录需要在运行时比较地址后才能完成逐资源闭环。文档不会把布局相似直接写成同一资源已实测。

因此，新后端的正确策略不是照搬 Xbox 的隐式统一内存访问，而是：保留 GPU 模拟与 GPU 视觉路径，给物理采样增加显式、三槽、带 fence 的 readback。

## 1. 名称与入口

| 名称/地址 | 已确认作用 | 证据等级 |
|---|---|---|
| `water_compressed_simulation_rendertarget`，字符串 `0x14677AA5C` | compressed simulation 的 shader/render-target 配置 | B：xref `0x141E49166` |
| `water_heightpass`，字符串 `0x14677B5BE` | 水面高度 pass 注册 | B：`0x14262091C` |
| `0x142625EE0` | compressed-simulation 的高层渲染 pass；绑定资源并发出 Compute 调度 | B：函数体 |
| `0x140BA1240` | 单组模拟调度与结果有效标志设置 | B：函数体 |
| `OceanWaveSim`，字符串 `0x14677FB61` | 另一套水面 Compute 技术 | B：`0x141FE2E17` |
| `0x140294DB0` | `OceanWaveSim` 的多资源 Compute 调度 | B：函数体 |
| `0x140047640` | CPU 侧按坐标采样模拟结果 | B：函数体 |
| `0x1453F7B90` | 返回 `(dword_147B99A38 + 1) % 3` | B：函数体 |
| `0x141FB4F70` | 推进三槽帧索引 | B：函数体 |

`WaterVelZ0..2` 的引用主要是纹理创建和 shader 绑定，未发现对应 CPU 读取。它不能仅凭名称被认定为物理回读缓冲。通用 `DownloadedTexture` 路径属于纹理下载/工具代码，也没有证据表明它是水面物理的必经路径。

## 2. GPU 模拟路径

### 2.1 compressed simulation

`0x142625EE0` 从 `a1[5]` 取得模拟状态，完成输入资源准备、根签名/描述符绑定和多次 Compute 调度。高层渲染接口的 `+0x160`（十进制 352）收到 XYZ 三维任务数，例如：

- 一组按 `width >> 2`、`height >> 2`、`1` 调度；
- 另一组按 `width >> 3`、`height >> 3`、`1` 调度；
- 前后还有参数、资源状态和结果拷贝/组合步骤。

`0x140BA1240` 对每组资源绑定再次封装，并在调度后写 `record+0x30 = 1`。该写操作发生在 CPU 录制返回时，**不是 GPU 完成通知**。尺寸来自资源描述 `+0x38/+0x3A`；函数自行构造 32 字节常量：前两个 float 按 phase 取 `(0,1)` 或 `(1,0)`，随后写入 groupWidth=8、channelMask=0xFF、unitScale=1.0f，尾部保留字段不命名。

`0x142625EE0` 还包含输入/历史准备、一次 `>>2` 调度、两次上述 Batch、一次 `>>3` 调度和条件性结果组合，不能只用一个空泛“绑定后 Dispatch”代替。

`0x141FB4F70` 推进两个索引：`dword_147B99A38` 受暂停条件约束，`dword_147B99A3C` 始终推进并返回。此前只写单索引 `%3` 并不完整。

这意味着后端必须保留以下顺序，而不能只实现最终波形画面：

```text
输入/历史状态
→ 设置 simulation 参数和资源
→ Compute 生成/更新模拟结果
→ 标记 CPU 记录有效
→ 后续 GPU 视觉处理
→ 在帧边界完成 GPU 工作，之后允许 CPU 使用旧槽
```

### 2.2 OceanWaveSim

`0x140294DB0` 同样通过高层接口发出三维 Compute 任务，并对多组输入/输出资源进行绑定。`0x140F31440` 为三组资源调用该路径。当前交叉引用显示这些资源继续用于 GPU，未找到与 `0x140047640` 相同的 CPU alias 采样消费者。

初版桌面后端应让该路径保持 GPU-only。不要为了“保险”每帧把所有 OceanWaveSim 输出复制回 CPU，否则会引入没有原始需求依据的带宽和同步成本。

## 3. CPU 物理采样

### 3.1 记录与 alias

`0x142913410` 创建/初始化模拟资源，并在 `0x14291347B` 调用：

```cpp
record.cpuAddress = sub_144FD2940(resourceFields);
```

`sub_144FD2940` 先通过 `sub_144FD2980` 取得资源的地址；当地址位于 `qword_149E1DAD0` 所描述的 CPU 区域时，将其转换为 `lpAddress` 对应的 graphics alias。它与资源章节的 GPU VA/CPU alias 模型一致，不是普通 CPU malloc，也不是 `ID3D12Resource::Map` 调用。

CPU 查询 `0x140047640` 使用 120 字节记录和 56 字节的第二变体偏移。已确认或被直接使用的字段为：

| 偏移 | 语义 | 说明 |
|---:|---|---|
| `+0x08` | GPU 资源/视图 A | 被 GPU 调度与资源状态路径使用 |
| `+0x10` | CPU `float2*` alias | CPU 直接按 float2 采样 |
| `+0x20` | GPU 资源/视图 B | 被 GPU 调度与资源状态路径使用 |
| `+0x28` | 原生资源对象 / CPU 插值权重表 | GPU 绑定时是 `ID3D12Resource*`；CPU 查询时是权重表指针，不能单写成一种类型 |
| `+0x30` | 结果有效标志 | 录制后置 1；不是 fence |
| `+0x34` | 行步长相关值 | 经 `>>3` 转成 float2 元素的行跨度 |

`0x140047640` 的签名是 `(mode, queries, count, output)`，并不返回样本指针。逐地址复核确认它有三条采样路径：

- **mode 1**：每个查询的 `packedX bit0` 选择相隔 56 字节的记录变体。读取 `float2`，行跨度为 `rowPitchRelated >> 3`，并使用 8×8 `uint8` 权重项（每样本 64 字节）插值。
- **其他非零模式**：读取 `float2`，使用 8×8 `uint16` 权重项（每样本 128 字节）插值，不逐查询切换记录。
- **mode 0**：读取一对 `uint16`，行跨度为 `rowPitchRelated >> 2`，使用 4×4 `uint8` 权重项（每样本 16 字节）及原全局比例/偏置常量计算结果。

R32G32F 路径的索引核心为：

```cpp
index = ((record.rowPitchRelated >> 3) * (packedY >> 3)) + (packedX >> 3);
```

两个分量有序且相等时直接返回第一个；否则按坐标低位从 `record+0x28` 的权重表取值插值。`record+0x30 != 1` 时输出零。所有模式最后都执行 `abs(value) < +Inf` 检查，Inf/NaN 最终写零。完整公式、符号移位和各模式权重宽度见 `pseudocode/10_water_simulation.cpp`，不能只保留一次 float2 地址计算就声称函数已重建。

另一个物理查询入口 `0x14124DD00` 多次调用该采样函数并进行双线性/区域插值，证明这不是调试纹理下载，而是正常的坐标查询链。

当前已核对的一组 CPU 采样资源创建参数为 **64×64、`DXGI_FORMAT_R32G32_FLOAT`（数值 16）**，每个 texel 8 字节，理论未对齐数据量为 32768 字节、紧密行距 512 字节。其他水面纹理使用不同格式和尺寸，不能统一套用这个布局。

### 3.2 三槽与帧延迟

- `dword_147B99A38` 在 `0x141FB4F70` 中以 3 为模推进。
- GPU 侧使用 `sub_1453F67B0()` 取得当前槽。
- CPU 查询使用 `sub_1453F7B90()`，即 `(current + 1) % 3`。

因此 CPU 不直接消费刚录制的当前槽，而是使用三槽环中的旧结果。`record+0x30` 仅表示该槽曾经生成，不能证明本轮 GPU 已完成。原引擎的正确性依赖帧调度和槽间隔，而不是查询函数内的同步等待。

当前静态证据没有显示 CPU 查询主动等待某一条专用 water fence。新后端应显式增加每槽 fence，而不是仅凭相同的 `%3` 运算推断桌面 GPU 也已完成。

## 4. 桌面 D3D12 实现

### 4.1 推荐结构

为每一种真正需要 CPU 的模拟输出建立独立合同；不要给所有水面资源创建 readback。

```text
Simulation UAV texture（GPU 读写）
        │ Compute 完成后转为 COPY_SOURCE
        ▼
CopyTextureRegion / CopyBufferRegion
        ▼
ReadbackSlot[N]（READBACK heap，CPU 可映射）
        │ 每槽独立 frameSerial + fenceValue + valid
        ▼
fence 完成后由 CPU 物理查询使用旧槽
```

每槽保存：

- readback resource 与持久映射地址；
- source frame serial、queue/fence value；
- source origin/grid spacing 等坐标元数据；
- `recorded` 与 `completed` 状态；
- 原始资源行距和 placed-footprint 信息。

建议初版使用三槽，匹配当前 EXE。槽数不是通用 fence 规则，而是当前水面路径的实测合同。

### 4.2 录制顺序

1. Compute 输出保持 UAV 可写，不在 compute 后立即 Map。
2. 插入 `UNORDERED_ACCESS → COPY_SOURCE`。
3. 将模拟结果复制到该帧对应的 READBACK 槽；纹理所用的 placed footprint 行距必须满足 D3D12 对齐，不能假设等于 512 字节。
4. 恢复后续 GPU 工作所需状态。
5. 在提交 queue 上 `Signal(waterFence, slot.fenceValue)`，再推进 slot。
6. CPU 查询只读取 `GetCompletedValue() >= slot.fenceValue` 的旧槽；不满足时使用上一份已完成结果或原引擎规定的无效结果，不能静默等待整个 direct queue。

如果模拟在 compute queue、视觉和复制在 direct queue，需要额外的跨 queue 等待。不能只靠 CPU 端的 `%3` 索引保证两个 queue 的顺序。

### 4.3 映射与 CPU 数据生命周期

有两种可行实现：

- **直接使用持久映射的 readback 槽**：复制完成后 CPU 直接采样，避免额外内存复制；该槽在再次录制前必须确认没有 CPU 读者。
- **退休时复制到持久 CPU 数组**：fence 完成后将 readback 内容复制到物理查询缓存，查询只访问缓存。实现更简单，也便于多线程物理，但多一次 CPU memcpy。

初版可选第二种以便验证内容；稳定后可改第一种减少 CPU 拷贝。任何情况下都不得让 GPU 在物理仍读取旧槽时覆盖它。

### 4.4 不建议的默认方案

- 把 UAV 资源直接放 `D3D12_HEAP_TYPE_READBACK`，试图省掉 GPU copy；
- 在 default heap 上直接 Map，桌面 D3D12 不支持这种通用做法；
- 每次物理查询调用 `WaitForSingleObject`，把整个 GPU 队列 stall 到当前帧；
- 假设所有 PC 都有 Xbox 式统一内存和隐式 cache 同步；
- 用 upload heap 代替 readback heap；
- 把所有 OceanWaveSim/WaterVelZ 输出都每帧下载。

部分独显/核显可以设计 CPU-visible 自定义 heap 的零拷贝优化，但那需要单独验证 GPU 写可见性、cache 和性能，不能作为跨硬件基线。

## 5. Shader 与接口改造

- 当前 Xbox shader blob 不能直接当作桌面 DXIL 使用；需要还原 shader 语义并重新编译。
- 保留 Compute 输入、历史状态、扰动参数、dispatch 分组和输出布局。
- CPU 采样需要的格式与物理查询保持一致；已确认的该组为 R32G32F，不要为了压缩带宽擅自改成 R16F 或 packed 格式。
- `WaterVelZ` 和 `OceanWaveSim` 初版留在 GPU，只有动态证明 CPU 消费者后才增加 readback。
- `ReadbackManager` 应暴露 `RecordDownload`、`Signal`、`TryAcquireCompleted`，而不是简单提供阻塞 `Map()`。

## 6. 运行时补证与验收

为完成逐资源闭环，应在原后端动态记录：

1. `0x140BA1240`：记录参数对象及 `+0x08/+0x20/+0x28` 的资源地址、dispatch 维度和 `+0x30` 写入。
2. `0x14291347B`：记录资源地址及 `sub_144FD2940` 返回的 alias。
3. `0x140047640`：记录被读取的 alias、行跨度、槽索引和结果。
4. 对照同一帧的原生 resource GPU VA、readback VA 和 CPU alias，确认精确生产者/消费者对应关系。

新后端验收至少包括：

- readback 内容逐 texel 对照，包含行距填充；
- 物理高度/速度查询与原后端数值对照；
- 三槽无覆盖、无未初始化读取、无当前帧整队列 stall；
- 暂停、恢复、resize、地图切换和资源重建；
- compute/direct 跨 queue 与长时压力测试；
- GPU-only 的 OceanWaveSim 不产生额外每帧下载。
