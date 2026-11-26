# 交接：在 Windows 上继续这件事

写给接手的人。这里只有实测过的东西，没验证的会标出来。

---

## 一、这东西现在是什么状态

一个 Xbox Series 的 GTA VI 在 PC 上跑起来的兼容层。代码是几个开源 fork 拼的，游戏文件自备。

**能看到的**：菜单、加载画面、过场动画、HUD、小地图、罗盘、字幕、人脸有五官、车身有高光、白天有阳光、黄昏有渐变。角色、车辆、树、路灯、栏杆、城市灯带都正常。

**速度**：菜单和加载 30 fps，进世界之后 **0 到 1 fps**（一帧 800–4800 ms）。显卡全程闲着（`Usage 0%`，`nothing of ours queued 2244 ms`），时间全在 CPU 上。

**为什么这么慢**：这台机器 14 GB 内存、8 GB 显存，进世界后 `swap 12–14 GiB` 打满。上游那几个 fork 的运行占用是 23–27 GB 内存 / 9.2–9.6 GB 显存，测试机 32 GB + 12 GB 卡。**这不是调参能解决的，是差了一整个档。**

---

## 二、代码在哪

GitHub：`github.com/ljm-233/VIctory`，分支 `main`。

`main` 的构成：
- 基底是 `trustno1-coder/VIctory-NVIDIA`（`033be27`）—— relay、card steps、compose、渲染缩放、自家 upscaler、动态槽、脸的修复、NaN scrub
- 上面叠了 `Harveyweiii/victory-fork` 的 `local/combined-tested` 里的 **shader 层**（wave64 paired form、mesh 与 pixel 链接）
- 加自己的修复：watchdog patch、线程栈、`VICTORY_KEYS` 在 release 下可用、`used_kind`/`boolean` 的递归上限

另外两个分支留着备查：`line/merged-fxceless`（合过 fxceless 那 85 个提交的版本，没成主线）。

---

## 三、Windows 上怎么编

仓库自带 `rust-toolchain.toml`（1.98.1 + msvc），Windows 上装好 Rust 和 MSVC C++ 生成工具（要带 Windows SDK）就能：

```
cargo build --release --locked -p victory
```

产出 `target\release\victory.exe`，约 73 MB。

**三个坑，在 Linux 交叉编译时踩到的，Windows 原生编应该不遇到，但记下来**：

1. 系统 cargo 若没有 `x86_64-pc-windows-gnu` 的 std，要换 rustup 的 stable
2. `dlltool` 要在 PATH 里
3. **release 目录必须放 `d3dcompiler_47.dll`**，否则会去加载 wine 内建的那个，然后因为缺 `libvkd3d-*` 报 `c0000135` 起不来。这机器上那份是 4.4 MB

还有一个必须做的：**把 exe 的栈 reserve 从 2 MiB 改成 16 MiB**。PE 头里 `pe+24+0x48` 那个 u64。改之前跑一会儿就会栈溢出。

---

## 四、启动包怎么排

```
D:\GTA6测试\
├─ CoolFox启动包\
│  ├─ Play-VIctory.cmd
│  ├─ victory-3d-rtx-pareto-residency.exe
│  ├─ d3dcompiler_47.dll
│  └─ dxil.dll
└─ GTA6.Xbox.v1.0.2.1\
   ├─ game_scarlett_master.exe
   ├─ common.rpf / data.rpf / anim.rpf ...
   └─ 其余 69 项
```

`Play-VIctory.cmd` 里的默认值 `%~dp0..\GTA6.Xbox.v1.0.2.1` 正好指向这个位置，不用改。

游戏目录那 209 GB 是用 NTFS 硬链接从原目录建的，不占额外空间。

---

## 五、必须设的开关

没有手柄的机器，`VICTORY_KEYS` 是唯一能按 A 的手段。格式是 `<起>-<止>:键`，单位是**秒**，从输入层第一次被读的时候算：

```
VICTORY_KEYS=10-10.8:Enter,20-20.8:Enter,30-30.8:Enter,...
```

游戏在「选择账户 PLAYER」那一屏等 A，不按就一直 8 draws 不动。

这个开关原来是 `#[cfg(feature = "diagnostics")]` 包着的，release 构建里根本不存在。主线上已经把这个 cfg 去了，三处（`pad()`、`keys()`、`scripted()` 本身）。

其他关键开关：

```
VICTORY_NO_WATCHDOG=1        标题自己的 watchdog 会杀慢帧，这个必须开
VICTORY_PIPELINE_WORKERS=1   排查用；正常跑用默认
VICTORY_RELAY=0              见第六节
VICTORY_CARD_STEPS=0
VICTORY_SHADER_LOOP_LIMIT=1024
```

`VICTORY_SKIP_PIPELINES` 只在 diagnostics 构建里有，release 没有。

---

## 六、那个上传补丁（scarlett_upload_patch）

独立的 C++ DLL，不在 Rust 仓库里。源码在 `GTA6/patch_fix3`，作者给的原版加了自己改的七处，全带 `// LOCAL:` 注释。

**它做什么**：hook 游戏的 XS 对象调用，直接发标准 D3D12，绕过胜利那一层的显存管理。

**怎么装**：改名为 `D:\scarlett_upload_patch.dll`，注入器分两步——先 `LoadLibrary`，等 1.5 秒让加载器锁释放，再调它的导出 `ScarlettUploadInstall`（RVA `0x44f0`）。不要指望它 `DllMain` 里那个线程，会在加载器锁里卡死。

**基址**：胜利是 loader，游戏的 PE 是手动映射的，不在模块列表里。补丁需要它，从 `D:\scarlett_module_base.txt` 读，内容是 loader 日志里 `image mapped base=0x...` 那个数（去掉 `0x`）。

**配套要关掉胜利的六条 copy 路径**，否则两边各分一套显存，补丁会报 `E_OUTOFMEMORY`（`hr=0x8007000e`）：

```
VICTORY_RELAY=0 VICTORY_CARD_STEPS=0 VICTORY_JOIN_COPIES=0
VICTORY_WIDE_BACK=0 VICTORY_TEXTURE_NO_WAIT=0 VICTORY_CARD_REHASH_RUNS=0
```

**当前状态**：能装、能拦、能转发。日志里有 `hooks installed; native execution uses standard D3D12 methods only`，`CreateBuffer` / `ExecuteCommandLists` / `RecordCopy` 全程接管，**失败 0 次**。

**改过的七处**（都在 patch_fix3 里）：

1. 基址从文件/环境变量读，不靠 `GetModuleHandleW(nullptr)`（那拿到的是 loader）
2. 先建 native backend 并设 `active_`，再装 hook（否则安装期间的 Barrier 调用找不到层）
3. 增强 `Barrier`（槽位 146）翻成普通 transition —— 宿主只到 `ID3D12GraphicsCommandList`，92 次 `E_FAIL` 就是这么来的
4. `CreateHeap` 被拒（`E_NOINTERFACE`）时退回 `CreateCommittedResource`
5. `ResolveCpuAlias` 在 alias 区里找不到记录时返回地址本身，不返回 null
6. 重复注册、销毁未注册资源、没有 command list 的 barrier，全部改成容忍
7. `InlineHook::Install` 在做 `memcmp` 前先 `VirtualQuery` —— 那个 destroy hook 的地址（RVA `0x4FB0460`）在这个 build 的 `.text` 之外，直接读会崩，崩了连日志都没有

---

## 七、没解决的问题

**① 翻译器栈溢出（最要紧）**

```
thread 'victory-pipelines-N' has overflowed its stack
stack 0x37550000-0x38550000     ← 16 MiB,已经是最宽的
thread 'victory-pipelines-0' has overflowed its stack
```

溢出前最后翻译的是 `hash=32ed072b9f845048 user_data=5 samplers=0`（单 worker 时锁定的）。试过都无效：`used_kind` 递归加 64 层上限、`boolean` 加同样上限、worker 栈从 16 MiB 提到 256 MiB reserve。

256 MiB 还爆就是无界递归。而 Rust 的栈溢出是直接 abort，不走 SEH，`SetUnhandledExceptionFilter` 抓不到现场。日志里那个 `0x1401aa628` 每次都指向 `crash::report`，是崩溃处理器自己在二次爆栈，不是递归点。

**下一步该做的**：给翻译器每个递归函数入口塞栈指针检查（当前栈指针离起点超过 12 MiB 就返回 `Err`），这样最坏情况是那个 compute 程序翻译失败、它的 draw 被跳过，而不是整进程死。

**② 纹理刷新**

```
texture refreshes per frame, all threads: 4247.0 ms
copies: 2179.7 ms
```

这是进世界后最大的单项。是逐页比较 title 内存和 host 副本，纯 CPU 访存，和纹理压缩解码**无关**。

**③ 纹理解码的真相**

`crates/victory-formats/src/bcpack.rs` 的 `decode(bc, src, dst, tables)` 纯 CPU、**单线程**、没有任何 GPU 调用。

但**给它加多线程不值得**——实测过：BC1 一块 64.1 ns，其中打包只占 1.3–4.3 ns（2–6%）；BC7 一块 234.9 ns，打包 37.8 ns（16%）。83–97% 在 `lanes()` 的 rANS 状态机加四次随机表访问里。加了 8 线程，BC1 反而慢 16%。

真正的路：降 `bucket_of` 的表访问代价，或者按 lane 做两阶段前缀切分再并行。别再从打包阶段下手。

**④ 那个补丁还没证明比不装快**

装上之后失败 0、能转发，但没做过「装与不装」的 A/B 帧数对比。在当前这个 0–1 fps 的量级上，任何优化都会被别的瓶颈淹掉。

---

## 八、日志和证据在哪

linux 那台的 `GTA6/logs/`：`victory.log`（11258 行）、`scarlett_upload_patch.log`、`launcher.log`、`module_base.txt`。

截图看 `victory-fork-ref/docs/local/`：`newline-car-hud.png`（车里，HUD 全套）、`harvey-daylight-hud.png`（白天）、`pkg-lucia-dusk.png`（黄昏）。

各分支的评估写在 `victory-fork-ref/docs/`：`nvidia-fork-notes.md`、`harvey-fork-notes.md`、`upstreams.md`、`upstream-merge-notes.md`。

---

## 九、一句话

代码和管线都通了，画面是对的，卡在硬件。想在 Windows 上有意义地继续，**先加到 32 GB 内存**，其他所有优化在这之前都是在噪声里找信号。
