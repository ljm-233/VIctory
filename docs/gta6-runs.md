# GTA6 / victory 这轮跑出来的东西

时间：2026-11-25 17:29 – 18:05。三轮真跑，进到 gameplay，零崩溃。

## 一、先修掉的两个真 bug

**PR 701 会崩。** `crates/victory-gpu/src/texture.rs` 那个 `VICTORY_TEXTURE_BEHIND_GUARD`
改动（`Last::behind_guard` / `refresh_hashed`，180+/25−）打上之后每次跑一分钟内崩：
`0xc0000005 at dbghelp.dll+0x18d00, read 0xffffffffffffffff`。撤掉之后连跑 8 分钟、
953 draws、零崩溃。diff 留在 `/tmp/pr701/701.diff`，以后要用得先查这个崩溃。

**着色器缓存键把整个 exe 算进去了。** `shader_cache::build()` 原来哈希 59.7 MB 的
victory.exe，所以改一行 Rust 就丢光 1116 条翻译。现在有 `VICTORY_SHADER_CACHE_KEY`：

- 不设 / `exe` —— 原行为
- `translator` —— 只跟 `victory-shader` crate 走（`build.rs` 算 `VICTORY_SHADER_FINGERPRINT`），
  改 victory-gpu / HLE / apps 都不失效
- `loose` —— 永不失效，改翻译逻辑要 `VICTORY_SHADER_CACHE_CLEAR=1`

同时补了：`host_max_wave()` 进 key（上游 #1844）、`ENCODING` = `shader_cache.rs` 自身文本、
失败结果也缓存（`VSHE` magic，上游 #1089）、命中时不再回读刚写的条目（上游 #2055）。

实测：`hits=778 misses=0`。一条都没重译，开局直接跳过整个翻译环节。

## 二、三个性能开关，实测有效

| 开关 | 作用 | 实测 |
|---|---|---|
| `VICTORY_WIDE_REFRESH=1` | copies 的刷新分到多线程 | 3527 ms → 260 ms（162 MB），13 倍 |
| `VICTORY_TEXTURE_NO_WAIT=1` | 纹理刷新不等 "title 内存静止" 锁 | 365 ms → 0 ms；刷新总 540 → 41.7 ms |
| `VICTORY_CARD_BUFFERS_MB=2048` | 大 buffer 的写检测交给卡，CPU 不再比对 1.6 GB/帧 | 默认 1024，超出的部分只能 CPU 比 |

已写进 `/tmp/wine-fork.sh`，下次直接跑就带着。

## 三、队列退避改成按墙钟（已改，已编译，已跑）

原来的退避是按帧数：

- `draw.rs` 的 `most = (4000 / took).clamp(16, 256)` —— 一帧 5 秒时 16 帧 = 80 秒
- `descriptor/host.rs` 的 `later()` 里 `w.due = frame + (1 << looks.min(16)).min(most)`
- `RECHECK_FRAMES = [4, 12, 40, 120, 400]` —— 0.1 fps 下 400 帧 = 一个多小时
- 不完整时 `frame + 120`

改成：

- `Waiting.retry: Option<Instant>` + `is_due(frame, now)`，时间是硬条件
- `later()` 退 `1, 2, 4 … 2048 ms`，上限 `most`
- `most = took.saturating_mul(8).clamp(16, 4000)`（毫秒，八帧的量）
- `RECHECK_MS = [66, 200, 667, 2000, 6667]`（60 Hz 下原来那组帧数的毫秒等值）
- 不完整时 `frame + 1`
- `due()` / `census()` / `texture_slots_due()` 都收 `Instant`
- 单元测试改成按时间断言（`later` 后立即不出、睡过退避后出）

实测（`run-clock-full.log`）：

| | 加载期队列 | 内存 | gameplay |
|---|---|---|---|
| 改前 | 21779 slots | 10.5 GB | 0.2 fps，19191 due |
| 改后 | 1419 slots（14 倍） | 4.0 GB | 0.2 fps，20327 due |

**加载期是净改善，gameplay 没救。** 因为 gameplay 的帧时间不是队列造成的：

```
copies 1750.7 ms / 98.3 MB
copying back 2495.0 ms / 14.4 MB   ← 14 MB 花 2.5 秒
heap buffers 1135.4 ms
on the GPU 381.9 ms
```

回读 5.8 MB/s 是最离谱的一项。队列 20327 due 是这些慢的**结果**，每帧 4096 个 look
在 0.2 fps 下要 25 秒一轮，永远追不上新写入。

## 四、剩下的真瓶颈（没修）

三轮对比，单位都是每帧：

| | copies | textures | copying back | GPU | fps |
|---|---|---|---|---|---|
| 基线 | 3527 ms / 91.7 MB | 140 ms | 1514 ms | 606 ms | 0.1 |
| +WIDE_REFRESH +CARD_BUFFERS | 260 ms / 162 MB | 4976 ms / 17.9 MB | 558 ms | 105 ms | 0.2 |
| +NO_WAIT | 2690 ms / 141.7 MB | 87 ms / 7.4 MB | 2073 ms / 35 MB | 1440 ms | 0.2 |

波动很大，同一个开关两次跑能差 10 倍。但纹理队列是持续恶化的那个：

```
textures picked by index, the queue: 21488 slots, 19191 of them due
(the oldest written 2378 frames ago), 897 waiting for their memory,
612 uploaded in part
```

**病根：退避按帧数算，不按时间。**

`crates/victory-gpu/src/draw.rs:1750`

```rust
let most = (4000 / took).clamp(16, 256);
```

`descriptor/host.rs` 的 `TextureQueue::later()`：

```rust
w.due = frame + (1u64 << w.looks.min(16)).min(most.max(1));
w.looks = w.looks.saturating_add(1);
```

帧率 0.2 时，退 256 帧 = 21 分钟。队列里大量"title 永远不会写"的死 slot 就这么赖着，
每帧 `texture_slots_due(frame, 4096)`（`draw.rs:1732`）的 4096 个配额被它们吃掉，
真正该传的排不上。`oldest written 2378 frames ago` 就是这么来的。

两个改法，任选：

1. `later()` 改成按时间退避（`Instant` 记 deadline），跟帧率解耦
2. `looks` 加硬上限（比如 > 8 就从队列里删掉，标题再写时会重新 `written()` 入队）

## 五、着色器侧：已经到底了

- 2342 条转译，成功 2337，99.79%
- 失败的就 3 个，全在 `SHADER-TRANSLATION-TABLE.md` 第 6 节
- `f133e35efed2a48a` 是 wave64 硬限制（RTX 5060 只有 32 lane），不是缺陷
- 另外两个是描述符位被当数字读，`descriptor_data()` 已经按宿主视图尺寸重建 GFX10 布局，
  剩下重不出来的位（地址、format 字段）才拒
- 翻译成本实测：均值 2.35 ms，emit 占 82.5%，清理 pass 占 35%（`VICTORY_SHADER_OPT=0`
  时 2.35→1.54 ms），没有可以安全砍掉的浪费

## 六、现场文件

- `SHADER-TRANSLATION-TABLE.md` —— 1232 条 GFX10 opcode 的转译路由
- `shader-translation-table.csv` —— 同样内容的机器可读版
- `run-gameplay-0.1fps.log` —— 基线，953 draws 那次
- `run-fast-wide.log` —— 开 WIDE_REFRESH + CARD_BUFFERS
- `run-nowait-wide.log` —— 再加 NO_WAIT，337 draws 那次
- `run-retry-clock.log` —— 只改 `later()`，加载期队列 1419
- `run-clock-full.log` —— `later()` + `recheck` + `frame+120` 全改，343 draws 那次
- `vdata/cache/shaders` 141 MB，`hits=778 misses=0`

## 七、一个哑弹 dll 折掉三条路（已修）

`target/x86_64-pc-windows-gnu/release/d3dcompiler_47.dll` 之前是个 **105777 字节的 mingw 哑弹**，
只导出一个 `D3DCompile`，对**任何**输入返回 `0x80004005` 且不给 error blob。wine 的 app-dir
搜索让它先于 system32 被加载，于是项目里所有 `D3DCompile` 调用全静默失败：

| 用它编译的 | 后果 |
|---|---|
| `CARD_COMPARE`（host.rs:1829） | 卡上比对从启动就是死的 → 每帧 1671 MB 在 CPU 上做 blake3 |
| `indirect.rs` 的 `WORK` | `compile(WORK,...).ok()?` 静默 None → argument clamp 从来不工作 |
| `inplace.rs` 的 `shade()` | 同上 |

换成 Lutris d3d_extras 的**真 MS 编译器**（`/home/link/.local/share/lutris/runtime/d3d_extras/v2/x64/d3dcompiler_47.dll`，
4410168 字节，md5 `bc4646db3eebd03315cc8e22ec1ab90b`）之后，全盘 8 处都换了。结果：

```
the compare: D3DCompile, cs_5_1, the compare: done
the compare's shader written to victory-card-compare.cso bytes=2248
the compare's root signature, serialized bytes=152
CreateRootSignature … done / CreateComputePipelineState … done
the compare is made: what shaders write to host copies in the card's memory is looked for on the card
host copies in the card's memory: 1 copies 174 MB     （原 0 copies 0 MB）
```

哑弹备份在 `/tmp/d3dcompiler_stub_105777.dll.bak`。`cargo clean` 之后这个 dll 不会自己回来，
要重新 cp。

## 八、copies 的 70% 是页表在被换出（不是算法）

新加的常开计数（`host.rs` 的 `PROBE` + 每帧一行 `the copies' probes per frame`）量到的：

baseline（未改算法，gameplay）：

```
copies 4705.9 ms / 126.5 MB
  the source asked in 6352.34 ms (7681 VirtualQuery calls), 2105 of them said no
  the destination asked in 1293.48 ms (5251 VirtualQuery calls)
  the kernel's whole read 558.62 ms in 6496 calls, 2105 failed
  the fallback 308.03 ms in 200 copies; 11.4 MB written as zeros
```

**单次 `VirtualQuery` = 827 µs**，正常机器上 1 GiB region 只要 14.5 µs —— **57 倍**，
因为页表自己被换出去了，每次走查都在做磁盘 I/O。44% 的 range 有一个洞，于是还多付
`ReadProcessMemory` 整段失败 + `read_sparse` 再扫一遍。

**反直觉的结论：换成按页问（`K32QueryWorkingSetEx` 256 页一问）反而更慢。** 微基准里是
5.8 µs/256 页 + 1.85 µs/页，但真跑里**非驻留页的那个 1 字节读 = 320 µs**（一次真实换页 I/O）。
加载期 69 次页问花了 531 ms / 6.7 MB。所以那条路已经默认关掉（`VICTORY_READ_PAGES=1` 才开），
代码留着 + 5 个字节级单测，内存充裕时可用。

**这台机器上任何"每个 copy 问内核一次"的方案都要付一次磁盘 I/O。** 真要治只有一条：
让 victory 用自己的 reservation/commit 记账（`xmem` 的 `VirtualAlloc2`/`MapViewOfFile3`、
`inplace` 的记录、`watch::title_memory_remapped` 的失效钩子）替掉内核询问，
把 827 µs 变成一次查表。那是会话级改动，不是补丁。

## 九、机器本身在换页，这是所有数字的上限

```
free:  15 GB 总，已用 13.4 GB，可用 1.9 GB
swap:  24 GB 总，用了 12-17 GB
vmstat: si 3-143 MB/s, wa 9-14%, kswapd0 常驻
/proc/sys/vm/swappiness = 100        ← 极端值，有可用内存也照甩
/proc/sys/vm/vfs_cache_pressure = 200 ← 同样激进
```

`98 MB/帧 ÷ 50 MB/s ≈ swap 带宽`，不是算法带宽。页表、copy 的源数据、title 自己的内存
都在 swap 里打转。

**这一项不改，copies 上所有测量都是 I/O 噪声。** 需要的（要 sudo）：

```bash
sudo sysctl -w vm.swappiness=10
sudo sysctl -w vm.vfs_cache_pressure=50
```

另外 `obs` 常驻 729 MB + QuickSync，不同时录就关掉。

## 十、并发闸门的漏洞（已修）

`/tmp/game-lock.sh` 是"先 ps 再起"，中间有 2 秒窗口期，两个子代理各起一个 victory 同时跑过
（内存直接吃到 13/14 GB、free 只剩 0）。已在 `/tmp/wine-fork.sh` 开头补上全流程持锁：

```bash
exec 9>/tmp/wine-fork.flock
flock -n 9 || { echo "[wine-fork] 已有实例在跑（flock 占着）" >&2; exit 75; }
```

## 十一、卡上比对活着了，但只兑现 174 MB

真编译器到位 + `make_card_root()` 用 `D3D12SerializeVersionedRootSignature` 之后：

```
the compare's shader is compiled here from="D3DCompile" bytes=2248
the compare is made: what shaders write to host copies in the card's memory is looked for on the card
host copies in the card's memory: 1 copies 174 MB   （原 0 copies 0 MB）
```

`victory-card-compare.cso` 的 md5 和用真 d3dcompiler 单独编的完全一致，坐实走的是 D3DCompile
而不是嵌入回退。

**顺带挖出一个更阴的**：vkd3d-proton 的 `CreateRootSignature` **不校验** blob 前 4 字节是不是
版本号（memcpy 进 desc 就往下走，返回 S_OK）。所以老代码"拿 DXBC 头当 serialized root signature"
从来没被拒过 —— 那颗 RS 一直是垃圾，PSO 绑的 root layout 对不上，就算编译器好了 dispatch 也会
拿错 binding。`make_card_root()` 把这条隐患一起堵了，rt.exe 单独验证过 RS + PSO 在 vkd3d-proton 上
S_OK 且 dispatch 结果正确（store → compare 全 0 → 改一字节 → `differs 1 0 0 0`）。

**但 `VICTORY_CARD_BUFFERS_MB=2048` 只兑现了 174 MB，能上卡的 copy 只有 1 个。**
原因不在这个开关：`card_for()`（`shadow.rs`）要求"上卡后 80% 的显存预算仍空闲"，
而贴图把显存吃满（5525 个 slot 到期那一帧），后面的 copy 全被这条规则挡掉。
**要把 1671 MB 搬上卡，得先解决显存预算那条，不然调这个开关没用。**

`pieces compared on the card` 一直是 0 是**正确行为**，不是坏了：`card_before` 只对
`Step::GpuWrite` 且 `shadow.carded()` 的 range 做比对，世界开始写之前没有这样的 range。

## 十二、能跑得更深了，但新崩点在 decoration bake

改完这一版跑到 **frame 6074**，卡在 "decoration bake" 阶段（5525 个 texture slot 到期、
184 个只上传了一部分、0.3 fps），随后进程消失，日志最后一行：

```
004c:err:sync:RtlLeaveCriticalSection section 0000000000035B80 "?" is not acquired
```

基线 `run-clock-full.log` 只到 frame **2773**（是被 TERM 的，不是崩的）。所以 6074 是以前从没
跑到的区域，更像"新到达的区域里有既存 bug"，不是新改动引入的。**这是下一个要打的地方。**

## 十三、N 卡接口：`VK_NV_raw_access_chains` 是真的

启动脚本里一直有 `VKD3D_DISABLE_EXTENSIONS=VK_NV_raw_access_chains`，把它拿掉之后：

| | GPU 时间 | copies | 帧时间 | draws 峰值 |
|---|---|---|---|---|
| 禁着（PR 701 那轮） | 9327 ms | 6035 ms | 16420 ms | 6863 |
| **开着** | **2926 ms** | 5588 ms | **10192 ms** | **7681** |
| 开着 + `VICTORY_INPLACE=0` | 3460 ms | 8602 ms | 14898 ms | 6923 |
| 开着 + `VICTORY_CARD_RESERVE=1` | **608 ms** | 7572 ms | 10212 ms | 5811 |

**GPU 时间降到三分之一，帧时间降四成，还能多跑 800 个 draw，零崩溃。**

vkd3d 的 CHANGELOG 就写着「Significantly improves GPU performance on NV GPUs in some games」——
它把 buffer 访问编成链式，省掉SSBO 每次的地址计算和边界检查。

这条也是唯一一处我们主动禁掉的 NV 扩展。已经改成默认开（`VKD3D_DISABLE_EXTENSIONS=` 空）。

**顺带否掉一个猜想**：`VICTORY_INPLACE=0`（不让 GPU 直接读 title 内存，走显存）**更差**——
14898 ms vs 10192 ms。原因在 probe 行里：关掉之后 `the source asked in 7883 ms / 17738 VirtualQuery`
又回来了，省的拷贝比不上多出来的预扫描。所以 in-place 要留着。

## 十四、`vkd3d` 里还能用的 N 卡扩展

```
EXT_descriptor_buffer          支持（默认就开）
EXT_mutable_descriptor_type    支持
EXT_memory_budget              支持
NV_device_generated_commands   不支持（vkd3d 没实现）
NV_raw_access_chains           支持，已开 ← 上面那三倍
```

`device_workarounds.c` 里还有一堆 NV 专属分支（`minStorageBufferOffsetAlignment=4`、
swapchain_maintenance1 的驱动 bug、`shader_float_controls2` 被禁），都是 vkd3d 自己按
`VK_DRIVER_ID_NVIDIA_PROPRIETARY` 判断的，不用我们管。唯一值得确认的是
「mesh shader 有但 barycentrics 没有就全局禁用 mesh shader」那条 workaround ——
我们的日志是 `host mesh shader support mesh=true`，说明 N 卡这代同时有
`KHR_fragment_shader_barycentric`，没触发。

### 四组的结论

GPU 时间一路压到 608 ms（从 9327），帧时间只从 16420 降到 10192 就不动了 ——
**地板是 copies，不是显卡**。开着 RESERVE 时 GPU 更快（608 ms）但 copies 更差（7572 ms，
它把纹理的显存份额抢了，纹理队列 22992 个 due 开始反噬），draws 峰值也从 7681 掉到 5811，
净零。所以 RESERVE 不加。

**能做的 N 卡接口已经做完了**：`VK_NV_raw_access_chains` 默认开、`VICTORY_INPLACE=2` 保持、
`VICTORY_CARD_RESERVE` 不开。剩下的 160 MB/帧搬不动，是 swap 的事，不是显卡的事。

## 十五、跟进上游：没走 merge，改成一个一个重做

上游 `Marvisak/VIctory` 比我们的基准（`ffe310e`）多 **319 个 commit**。其中对我们有用的是最新两条：

- `cd94d35` — shaders 穿过 view 写的页打标记，copy-back 只比对打了标记的页。
  上游自己的数字：每帧 2.8-4.5 GB 快照 → 只比标记过的页，1.5-1.7 s 的过场帧里省 0.45-0.7 s
- `940d32f` — 被标记的页 copy-back 时不给自己存一份

### 为什么没整 branch merge

试过，11 个文件 35 块冲突，解到 8 个时发现三个是**模型级**冲突，不是文本级：

| 模块 | 我们/1k-fresh 的 | 上游的 |
|---|---|---|
| `watch.rs` | `writes()` 自由函数 + `Journal.written` 用 `HashMap`（省每页 log-n 插入） | 整个重写成 `Tracked` 句柄模型，`writes()` 这个函数**没有了**，新增 `Changed`/`Saved`（给 view-marks 存页） |
| `texture.rs` | 问 write watch，省下每帧 10.6 MB 的 hash | 每帧算 hash（`last.hash`），`refresh_timed` 返回从 `usize` 改成 `(usize, bool)` |
| `shadow.rs` | 用户态拷贝那条线，207 行 | view-marks + copy-back 标记，377 行 |

两边对同一个问题给了相反的答案。硬合会丢掉已验证的优化，所以中止了，改成在最新上游上**一件一件重做**。

### 重做出来的（分支 `redo/on-upstream`，base `940d32f`）

```
d3c1fc2 gpu: copy title memory in user space where both ends are there
34de511 gpu: the shader cache key includes the wave64 switches
0aace67 shaders: fix the hand-written module datalayout
db5e52a shaders: a heap handle's identity includes its component type
f48665d shaders: emulate wave64 compute on 32-lane host waves through LDS
940d32f (upstream) GPU: a marked page's copy-back does not save the page for its own copy
```

五个都编译通过，`cargo build --release` 干净。

**四项是我们自己那套，上游已经有了等效或更好，不用重做**：

| 我们做的 | 上游 |
|---|---|
| translator key（`VICTORY_SHADER_CACHE_KEY`） | `build.rs` 的 `VICTORY_SHADER_SOURCE_HASH` + `ENCODING`，本来就是 translator-keyed |
| 纹理队列退避改墙钟 | 上游把「按索引取纹理」那个队列**整个去掉了**（`TextureQueue`、`texture_slots_due`、`heap_textures` 都不在） |
| PR 701（behind guard 用 hash 判断） | 上游纹理本来每帧 hash，不会重复上传 |
| card compare（97 copies / 941 MB） | 上游的 view-marks 更彻底，不用把 copy 搬上卡 |

### 跑起来的第一个问题：缺 in-place

`/tmp/run-redo.log` 跑到 12 draws 时崩在 `vkAllocateMemory` 里
（`Exception 0xc0000005 in Unix call`）。日志里：

```
host textures: 2042 (5650 MB, 162 MB of them used by lists)
host pipeline library saved ... made=656 created=84238 distinct=83548
shader translated ... ms=2795 / 2906 / 3840 / 3517 / 3256（最长 23950）
```

- `created=84238` **不是 bug**：`note_created`（`make.rs:213`）在标题创建 pipeline 时就计数，
  GTA6 这三分钟真的要了 8.4 万个 PSO，造出来 656 个，其余没被 draw 用到
- 单次翻译 2.4-23.9 秒，**比 fork 版的 2.35 ms 慢三个数量级** —— view-marks 的代价，要看
- **真因是缺 in-place**：整份日志里没有 `title memory in place` 一行，`VICTORY_INPLACE=2` 被忽略。
  fork 版有 364 MB title 内存直接给 GPU 读、不占显存；上游版全要拷贝，
  5650 MB 纹理 + pipeline 内存就顶爆了 8 GB 的卡

`inplace.rs` 现在在 `git stash`（1k-fresh 的「VA line」那 62 行未提交改动），
基于旧上游写的，拿回来要重新适配 —— **这是下次的第一件事**。

## 十六、上游 PR 里对症的（已经拿了四个）

`gh pr list --repo Marvisak/VIctory --state open` 里有几十条，四条正好打在我们的问题上：

| PR | 规模 | 打在哪儿 |
|---|---|---|
| **#2389** streamed textures 只占标题实际持有的 mip 等级 | +870/−15，6 文件 | 显存爆。开关 `VICTORY_LEVELS_ON_DEMAND=1`。实测纹理 **5650 MB → 4773 MB**（3220 个纹理） |
| **#2349** 更便宜的 title 内存拷贝、refresh、copy-back | +633/−135，3 文件 | copies。**它不做页表预扫描**：直接 `copy_nonoverlapping`，靠 SEH（`victory_copy_fast` + `recover_copy`）把 fault 重定向回内核拷贝。有洞的范围现在只付一次 fault，不是四遍页表扫描 |
| **#2363** descriptor 的 dword 当数据读（record count 和 format） | +58/−19，3 文件 | 我们那 5 个失败 shader 里的 3 个 |
| **#2347** spilled lane masks、Atom::Phi 回边、carry masks | +255/−74，2 文件 | `f133e35efed2a48a` 那个 lane mask |

四个都打上、编译通过（分支 `redo/on-upstream`）：

```
b799841 GPU: streamed textures take video memory only for the levels the title holds (#2389)
fc4c666 GPU: cheaper title memory copies, refresh batches and copy-back (#2349)
6860e8d Shaders: track spilled lane masks, Atom::Phi back-edges, and carry masks (#2347)
fccb158 GPU: descriptor dwords read as data — the record count and the format (#2363)
d3c1fc2 gpu: copy title memory in user space where both ends are there
34de511 gpu: the shader cache key includes the wave64 switches
0aace67 shaders: fix the hand-written module datalayout
db5e52a shaders: a heap handle's identity includes its component type
f48665d shaders: emulate wave64 compute on 32-lane host waves through LDS
940d32f (upstream) GPU: a marked page's copy-back does not save the page for its own copy
```

`#2349` 里那条 SEH 拷贝正是我原来想的方案 C，比手写的 `readable`/`writable` 预扫描强，
所以把它自己那版覆盖掉了（`d3c1fc2` 被 `fc4c666` 替换）。

### 跑起来的结果

**过了原来 `vkAllocateMemory` 崩的那个点**（上次 12 draws / 3 分钟崩，这次跑了 9 分钟）。
纹理稳在 4773-4788 MB。

但**新崩点在 `vkCreateComputePipelines`**：

```
WARN host pipeline refused error=0x80004005 shaders=[compute 7cb338ecaeac0aa9]
WARN host pipeline refused error=0x80004005 shaders=[compute 5d061e6aed59c57d]
026c:err:vulkan:vkCreateComputePipelines Exception 0xc0000005 in Unix call.
```

而且 `shader does not translate` 从 fork 版的 3 个涨到 **12 个**，新的拒绝文本是
`0x4d0: a descriptor neith...` 和 `0x14dc: a lane mask read ...` ——
**`#2363` 或 `#2347` 引入了新的翻译拒绝路径**，vkd3d 拒了 pipeline 之后崩。

下次的第一件事：逐个撤 `#2363` / `#2347` 二分，看 12 个失败降到几个。

## 十七、游戏退出后内存和显存不释放（已解决）

现象：游戏进程没了，`free` 还是 14.8 GB 用着、`nvidia-smi` 还剩 6775 MiB 显存。

原因：wine 的 `explorer.exe`、`qq` 之类的实例在游戏退出后继续挂着 `/dev/nvidia0`
和 `/dev/nvidia-ctl` 的映射。`nvidia-smi --query-compute-apps` 看不到它们（它们不是 compute 进程）。

**解法：`wineserver -k`**。实测释放 7 GB 内存 + 6.7 GB 显存（6775 MiB → 52 MiB），
内存 14879 MB → 7599 MB。

脚本在 `/tmp/cleanup-gta6.sh`，每轮跑完执行一次。

另外 `/tmp` 里之前子代理留下的诊断目录（`diag-capture2` 340 MB、`diag-capture3` 237 MB 等）
占了 1.3 GB，也一并清了。

## 十八、进世界了：四个开关把 pipeline 队列排空

作者主线 `line/main`（PR #2 + #3，比我们推的分支多 208 个 commit）加四个开关：

```bash
export VICTORY_INDIRECT_WINDOW=1
export VICTORY_TEXTURE_NO_WAIT=1
export VICTORY_SHADER_LOOP_LIMIT=1024
export VICTORY_TEXTURE_ROOM_PERCENT=82
```

**结果**：

| | 之前 | 这轮 |
|---|---|---|
| `being made` | 1274-1830，**永远不动** | 1404 → **4**（排空） |
| draws 峰值 | 6863 | **10537** |
| 帧率 | 1.9 fps，5.5 秒一帧 | 22-25 fps（加载期），进世界后 0.1 fps |
| copies | 1750 ms / 98 MB | 2-4 ms / 1.4 MB（加载期），进世界 1180 ms / 92 MB |
| 崩溃 | 常崩 | 0 |

draws 的曲线：11-15 持续到 16:54:22，然后 16:54:54 一跳 **1571 → 7252 → 10537**。

**之前最高的 6863 是从没进过世界的峰值**（那时候是掉帧前的瞬间），这轮是真正的世界帧。

`VICTORY_INDIRECT_WINDOW=1` 最可能是解 `being made` 卡死的那个：间接执行的窗口在等，和 pipeline 卡死的表现一样。但没做 A/B，不确定是它单独还是四个一起。

**还有 2038 个 draw 在等 pipeline**（最后一帧 `dropped 2038 without a host pipeline`），说明主队列排空了但新 pipeline 还在持续要。这是下一步：把 PR #3 那个「每条待编译管线只在首次请求的帧等待」的逻辑用上。

## 十九、用户给的 xss 配置里只有四个在这条线上存在

他给的 12 个开关，`line/main` 里只有 4 个：

| 存在 | 不存在（在那个领先分支 `queue2.rs` 那套里） |
|---|---|
| `VICTORY_INDIRECT_WINDOW` | `VICTORY_JOIN_PARTS` |
| `VICTORY_TEXTURE_NO_WAIT` | `VICTORY_BACK_OWN_VIEW` |
| `VICTORY_SHADER_LOOP_LIMIT` | `VICTORY_ASYNC_SUBMIT` |
| `VICTORY_TEXTURE_ROOM_PERCENT` | `VICTORY_QUEUE2` |
| | `VICTORY_COPY_PLAIN` |
| | `VICTORY_MERGE_GAP` |
| | `VICTORY_MERGE_RUNS` |
| | `VICTORY_LEVELS_ON_DEMAND` |

那 8 个要在 2222.rar 那个分支里才有（`queue2.rs` 1047 行双 copy queue、`drvwatch.rs`、`cardmeter.rs`）。
