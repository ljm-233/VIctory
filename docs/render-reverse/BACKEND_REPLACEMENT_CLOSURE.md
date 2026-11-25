# 完整渲染后端替换：接口闭合矩阵

本文按“能否替换后端”评估，而不是按是否知道每个私有名称评估。状态分为：

- **直接闭合**：当前 IDB 已给出替换所需的字段、参数和生命周期。
- **回退闭合**：可强制走已确认的 XS 函数表慢路径，不必实现私有快路径。
- **非阻塞未知**：未调用接口或仅影响优化/原始名称，不影响当前后端替换合同。
- **待实机**：静态合同已覆盖，但仍需实际 Hook 和渲染验证。

## 覆盖矩阵

| 子系统 | 状态 | 替换所需的已确认合同 |
|---|---|---|
| Device、Queue、GCL 创建 | 直接闭合 | Device11、1 direct + 4 compute queue、allocator/list 配对、ResetX/Close、队列上下文和池生命周期 |
| `CreateDeviceFlags=0x20` | 非阻塞未知 | 原值已记录但名称未知；后端不能擅自映射成其他平台标志，初版替换可不启用该私有优化 |
| 内存类型与 heap | 直接闭合 | `RageMemoryType::{Default,Upload,Readback,Custom}={0,1,2,3}`；upload/readback committed 的 heap type、CPU/pool UNKNOWN 和节点掩码均为实测 |
| GPU VA 与 CPU alias | 直接闭合 | graphics reservation、CPU alias、placement 地址转换、页池注册、六种 packed page commands、free-list、scatter/batch 和 VA 去重均已恢复 |
| subtype/分配器 | 直接闭合 | bits0..1 选择分配器，bits3..4 选择 memoryType；bit2 决定地址所有权；扩展/主路径固定 variant 1/3 并同槽释放；memoryType 只影响 128 字节对齐 |
| `RageFormatDesc` | 直接闭合 | 12 字节完整布局；format 直接为 DXGI_FORMAT，dimension 0→TEXTURE1D、3→TEXTURE3D、其他→TEXTURE2D；sampleMode 使用 17 项 DWORD 表 |
| 原生 descriptor | 直接闭合 | heap increment、CPU/GPU handle 转换、external CPU base 和 SRV/UAV/RTV/DSV 创建路径 |
| 软件 CBV | 直接闭合 | 16 字节 `HardwareCbvDescriptor`：`taggedAddress=VA+(1<<52)`、`recordCount=AlignUp(size,16)/16`、`flags=0x2403C` |
| Graphics PSO | 直接闭合 | 688 字节、15 个子对象的完整 stream；FindOrCreate 与编译期 hash 已分别恢复；输入元素数组独立存活到创建返回；AS/MS 复用 VS/HS 槽 |
| Derived PSO | 直接闭合 | 332 字节 desc，顺序为 rasterizer、depth/stencil、可选 depth options、blend；没有其他派生类型 |
| Indirect draw / compute root | 回退闭合 | 5 个 command signature、9 个 ExecuteIndirect 包装和 compute root/PSO 变体已定位；独立/共享参数和 DispatchL2 均已区分，不能按 DrawIndexed 处理 |
| Query / DXR / BVH | 创建与描述闭合，执行按命中 | QueryHeap、StateObject N+9、BVH 80 字节描述、update/compaction、COMPACT、序列化头、UAV/COPY_SOURCE、packed geometry/instance 和 Morton 均已恢复；DXR dispatch 与 direct mesh 未证实，不虚构执行 |
| GCL 内联 Draw/IA/root | 回退闭合 | 已恢复主要 packet 和选择规则；将 put/soft limit/allocation end 设为不满足容量检查即可全部调用 XS 方法 |
| PSO 内联绑定 | 回退闭合 | 快路径复制原生 PSO `+0x10..+0x2B` 共 28 字节；limit 不满足时改调 `SetPipelineState`，替换初版无需伪造该私有 PSO 数据 |
| Barrier | 直接闭合 | GCL146，16 字节 group、40 字节 buffer、64 字节 texture；组/资源容量、完整字段生成和 flush 边界已恢复 |
| Render-target 分块 transition | 直接闭合 | RT/DS 设置、范围格式、状态编码、10-bit 记录和单记录/全不同/混合处理均已恢复 |
| 批量提交 | 直接闭合 | 96 批环、80 字节记录、最多 256 个 GCL、共用 `flushAndExecutePooledList`、回调和完成 fence |
| Frame token 与 fence | 直接闭合 | token 负责帧归属，fence 负责 GPU 完成；`PROCESS_TOKEN_ON_TIMEOUT` 允许提前构建，不能互相替代 |
| Present | 直接闭合 | 80 字节记录内联最多两个 plane、完整源/目标矩形、显示间隔、immediate 和完成信号 |
| Deferred/延迟回收 | 直接闭合（行为） | 48 个注册项顺序和直接行为已分析；后端按原调用执行即可，不要求知道每个原始 pass 名 |
| 未调用的 XS 槽位 | 非阻塞未知 | Device126、未使用的 GCL 槽和文档推导顺序不属于当前 title 的运行时合同；若后续动态命中再补 |
| 私有 packet 全部位域 | 非阻塞未知 | 初版统一走 XS 慢路径；只有恢复原生快路径优化时才需要继续展开 |

## 替换实现必须保持的顺序

1. 建立 Device/Queue/Resource/Descriptor 兼容对象，保留 XS 显式 `this` ABI 和引用生命周期。
2. 初始化 VA、CPU alias、页池和资源地址所有权；不能只给每个资源分配普通 Windows 内存后返回原 GPU VA。
3. 初版让 GCL 的 put、soft limit 和 allocation end 不满足所有内联容量检查，统一进入 XS 方法；具体字段和 ResetX 处理见 `FORCED_SLOW_PATH.md`。
4. 软件 CBV 仍按上述 16 字节公式生成；它不会被 GCL limit 回退覆盖。
5. PSO 使用完整 stream/derived desc；PSO 内联绑定同样通过 limit 回退到 `SetPipelineState`。
6. 保持批量提交顺序、token/fence 分工、Present 记录持久性和延迟回收时序。
7. 完成实机验证后，再逐项启用内联 packet、原生 PSO 数据和私有内存优化。

## 当前边界

**本轮逐地址复核后，当前 title 已识别的渲染接口层达到完整静态合同闭合。** StateObject、普通设备/资源/提交、PSO hash、完整 root-signature bitstream、packed page commands、packed geometry/instance 和分块 render-target transition 均已恢复，详见 `RENDER_REMAINING_AUDIT.md`。全 `.text` 审计仍未证实 DXR dispatch、direct DispatchMesh 或 Query 执行，这些未调用路径不虚构。尚未进行 Windows Hook、PIX capture、逐帧资源对照或全渲染 pass 动态覆盖。

验证时应优先检查：

- upload/readback 的 CPU 可见性和 alias 地址是否一致；
- 软件 CBV 的 16 字节描述是否逐字节匹配；
- 强制慢路径后 Draw/PSO/root binding 是否仍全部发生；
- token 提前生成时 Present 是否仍等待正确 frame origin；
- fence 完成前是否没有提前回收资源或复用 Present 记录。

水面专题另见 `WATER_SIMULATION_READBACK.md`：Compute、CPU alias 和三槽合同已恢复，但每次 dispatch 的原生资源与每次物理查询的 alias 仍需动态地址对照，不纳入上述核心静态闭合的过度断言。
