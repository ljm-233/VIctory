# 着色器转译表 — GFX10 → DXIL（victory-shader）

这份表是**给转译器用的**：游戏发来的 GFX10 机器码里每一条指令，落到 `emit/lower.rs` 的哪个
处理器、走显式分支还是通用路径、还是明确拒绝。表由源码生成，不是统计报表。

生成方法见文末。opcode 名来自 `gfx10/opcodes.rs`（Mesa `aco_opcodes.py`, tag mesa-22.3.0, GFX10 列），
共 1232 条。

## 1 路由：encoding → Fields → 处理器

`gfx10::decode()` 解出 `Instruction{encoding, opcode, words}`，`gfx10/fields.rs` 按 encoding 把它
包成 `Fields` 变体，`emit/lower.rs:4824` 的 `match &inst.fields` 决定处理器。

| encoding | Fields | 指令数 | 处理器 | 位置 |
|---|---|---:|---|---|
| SOP1 | `Sop` | 66 | `sop()` | lower.rs:4982 |
| SOP2 | `Sop` | 51 | `sop()` | lower.rs:4982 |
| SOPK | `Sop` | 27 | `sop()` | lower.rs:4982 |
| SOPC | `Sop` | 18 | `sop()` | lower.rs:4982 |
| SOPP | `Sopp` | 37 | `sop()` / 内联 | lower.rs:4826-4848 |
| SMEM | `Smem` | 84 | `smem()` | lower.rs:5523 |
| VOP1 | `Valu` | 81 | `valu()` | lower.rs:6644 |
| VOP2 | `Valu` | 50 | `valu()` | lower.rs:6644 |
| VOPC | `Valu` | 190 | `valu()` | lower.rs:6644 |
| VOP3 | `Valu` | 116 | `valu()` | lower.rs:6644 |
| VOP3P | `Valu` | 28 | `valu()` | lower.rs:6644 |
| VINTRP | `Vintrp` | 3 | `interp()` | lower.rs:9206 |
| DS | `Ds` | 154 | `ds()` | lower.rs:4387 |
| MUBUF | `Buffer` | 75 | `buffer()` | lower.rs:10309 |
| MTBUF | `Buffer` | 16 | `buffer()` | lower.rs:10309 |
| MIMG | `Image` | 103 | `image()` / `bvh()` | lower.rs:9520 / 4857 |
| FLAT | `Flat` | 54 | `flat()` | lower.rs:1656 |
| GLOBAL | `Flat` | 57 | `flat()` | lower.rs:1656 |
| SCRATCH | `Flat` | 22 | `flat()` | lower.rs:1656 |

`Exp` 没有 opcode 表（固定 4 个目标），走 `export()`。

## 2 覆盖：每族怎么处理

`显式` = `lower.rs` 里按**完整指令名** match；`通用` = 按族前缀/后缀统一推导；
`专用` = 族内单独一条路径；`空` = 不产生宿主指令（控制流另处处理）；`拒绝` = 返回 `Why`。

| 族 | 合计 | 显式 | 通用 | 专用 | 空 | 拒绝 |
|---|---:|---:|---:|---:|---:|---:|
| SOP1 | 66 | 18 | 48 | 0 | 0 | 0 |
| SOP2 | 51 | 44 | 7 | 0 | 0 | 0 |
| SOPK | 27 | 6 | 9 | 12 | 0 | 0 |
| SOPC | 18 | 0 | 0 | 18 | 0 | 0 |
| SOPP | 37 | 0 | 0 | 17 | 20 | 0 |
| SMEM | 84 | 0 | 0 | 16 | 0 | 68 |
| VOP1 | 81 | 56 | 25 | 0 | 0 | 0 |
| VOP2 | 50 | 44 | 5 | 1 | 0 | 0 |
| VOPC | 190 | 0 | 0 | 190 | 0 | 0 |
| VOP3 | 116 | 72 | 39 | 5 | 0 | 0 |
| VOP3P | 28 | 3 | 6 | 19 | 0 | 0 |
| VINTRP | 3 | 0 | 0 | 3 | 0 | 0 |
| DS | 154 | 8 | 145 | 1 | 0 | 0 |
| MUBUF | 75 | 8 | 0 | 67 | 0 | 0 |
| MTBUF | 16 | 0 | 0 | 16 | 0 | 0 |
| MIMG | 103 | 0 | 0 | 103 | 0 | 0 |
| FLAT | 54 | 0 | 54 | 0 | 0 | 0 |
| GLOBAL | 57 | 0 | 57 | 0 | 0 | 0 |
| SCRATCH | 22 | 0 | 0 | 0 | 0 | 22 |
| **合计** | **1232** | **259** | **395** | **468** | **20** | **90** |

## 3 拒绝清单（90 条）

这 90 条 opcode 进来就返回 `Why`，没有宿主指令。两个来源：

### 3.1 SCRATCH 22 条 — `Err("flat memory")` (lower.rs:4867)

`Fields::Flat` 只在 `name.starts_with("global_")` 时进 `flat()`；`scratch_*` 落不到，直接拒。
私有 scratch 不是标题 shader 会用的路径。

`scratch_load_ubyte`, `scratch_load_sbyte`, `scratch_load_ushort`, `scratch_load_sshort`, `scratch_load_dword`, `scratch_load_dwordx2`, `scratch_load_dwordx4`, `scratch_load_dwordx3`, `scratch_store_byte`, `scratch_store_byte_d16_hi`, `scratch_store_short`, `scratch_store_short_d16_hi`, `scratch_store_dword`, `scratch_store_dwordx2`, `scratch_store_dwordx4`, `scratch_store_dwordx3`, `scratch_load_ubyte_d16`, `scratch_load_ubyte_d16_hi`, `scratch_load_sbyte_d16`, `scratch_load_sbyte_d16_hi`, `scratch_load_short_d16`, `scratch_load_short_d16_hi`

### 3.2 SMEM 68 条 — `Err("a scalar memory read as data")` (lower.rs:5723, 5731, 5877)

`smem()` 只认 `s_buffer_load_dword*`（1/2/4/8/16 个 dword，lower.rs:5540），其余交给
`descriptor_data()`；后者只认 `s_load_dword*`（lower.rs:5725）。两条都不认的，就是这张表。

- `s_scratch_load_dword`, `s_scratch_load_dwordx2`, `s_scratch_load_dwordx4`, `s_scratch_store_dword`, `s_scratch_store_dwordx2`, `s_scratch_store_dwordx4`
- `s_gl1_inv`, `s_dcache_inv`, `s_dcache_wb`, `s_memtime`, `s_memrealtime`, `s_atc_probe`
- `s_atc_probe_buffer`, `s_dcache_discard`, `s_dcache_discard_x2`, `s_get_waveid_in_workgroup`, `s_buffer_atomic_swap`, `s_buffer_atomic_cmpswap`
- `s_buffer_atomic_add`, `s_buffer_atomic_sub`, `s_buffer_atomic_smin`, `s_buffer_atomic_umin`, `s_buffer_atomic_smax`, `s_buffer_atomic_umax`
- `s_buffer_atomic_and`, `s_buffer_atomic_or`, `s_buffer_atomic_xor`, `s_buffer_atomic_inc`, `s_buffer_atomic_dec`, `s_buffer_atomic_swap_x2`
- `s_buffer_atomic_cmpswap_x2`, `s_buffer_atomic_add_x2`, `s_buffer_atomic_sub_x2`, `s_buffer_atomic_smin_x2`, `s_buffer_atomic_umin_x2`, `s_buffer_atomic_smax_x2`
- `s_buffer_atomic_umax_x2`, `s_buffer_atomic_and_x2`, `s_buffer_atomic_or_x2`, `s_buffer_atomic_xor_x2`, `s_buffer_atomic_inc_x2`, `s_buffer_atomic_dec_x2`
- `s_atomic_swap`, `s_atomic_cmpswap`, `s_atomic_add`, `s_atomic_sub`, `s_atomic_smin`, `s_atomic_umin`
- `s_atomic_smax`, `s_atomic_umax`, `s_atomic_and`, `s_atomic_or`, `s_atomic_xor`, `s_atomic_inc`
- `s_atomic_dec`, `s_atomic_swap_x2`, `s_atomic_cmpswap_x2`, `s_atomic_add_x2`, `s_atomic_sub_x2`, `s_atomic_smin_x2`
- `s_atomic_umin_x2`, `s_atomic_smax_x2`, `s_atomic_umax_x2`, `s_atomic_and_x2`, `s_atomic_or_x2`, `s_atomic_xor_x2`
- `s_atomic_inc_x2`, `s_atomic_dec_x2`

后果分两种：

- 结果**没人读**（`inst.writes` 的值只当描述符位用）→ 记成 `Val::Missing`，不报错（lower.rs:4872）。
  这是绝大多数情况：控制台编译器反复 `s_load_dwordx4` 取同一个描述符的 dword。
- 结果**被当数字读**（宽度、层数、记录数）→ 走 `descriptor_data()` 从宿主视图尺寸重建 GFX10
  布局（lower.rs:5705 注释那段），重建不出的位才是 `OTHER = "a descriptor's address or format read as data"`。
  实测只有 5 个 shader 撞到，见第 6 节。

## 4 特判规则表

`lower.rs` 里按名字分派的所有模式，按出现顺序。**转译器查不到显式条目时，落这里最后一行。**

| 模式 | 处理器 | 语义 | 位置 |
|---|---|---|---|
| `v_cmp_*` / `v_cmpx_*` | `compare(rest)` | 浮点/整数比较；`v_cmpx` 写 EXEC | 6655 |
| `v_pk_*` | `packed16(op)` | 双 16 位打包运算 | 6822 |
| `v_*_f16` / `_i16` / `_u16` / `_d16` | `valu16()` | 16 位 VALU，先试 | 6811 |
| `v_interp_p1*` / `p2*` / `mov` | `interp()` | 插值（p10/p20/p0） | 4850, 9281 |
| `v_mbcnt_*` | `valu()` | 波内计数，mask 安全 | 4788 |
| `v_cvt_f32_ubyte*` | 内联 | ubyte→f32 特判 | 7328 |
| `*subrev*` | 内联 | 反向减法 | 7548 |
| `*_ci_u32` | 内联 | 带进位输入 | 7551 |
| `s_cmp_*` / `s_cmpk_*` | `sop()` | 标量比较进 SCC | 5071, 5090 |
| `s_bitcmp*` | `sop()` | 位比较 | 5048 |
| `s_bcnt1_i32_*` / `s_ff1_i32_*` | `sop()` | 位计数/前导零，mask 安全 | 4786, 4936 |
| `s_waitcnt*` | 内联 | 等待折进使用点 | 4999 |
| `s_barrier` | 内联 | `barrier_1` + LDS/UAV fence | 4827 |
| `s_cbranch*` / `s_branch` / `s_setpc_b64` | `control()` | CFG 边 | decode 期 |
| `s_endpgm` | `control()` | 程序出口 | decode 期 |
| `s_buffer_load_dword{,,x2,x4,x8,x16}` | `smem()` | 常量缓冲加载 | 5540 |
| `s_load_dword{,,x2,x4,x8,x16}` | `descriptor_data()` | 描述符 dword 重建 | 5725 |
| `s_load_dword*` 从 CODE 参数 | `code_smem()` | 程序常量表 | 5971 |
| `global_load_*` | `flat()` | 全局读 | 2880 |
| `global_store_*` | `flat()` | 全局写 | 2902 |
| `global_atomic_*` | `flat()` | 全局原子 | 2918 |
| `global_load_lds` | `flat()` | 全局→LDS | — |
| `ds_read*` | `ds()` | LDS 读 + format/stride/d16 | 4594 |
| `ds_write*` | `ds()` | LDS 写 | 4614 |
| `ds_bpermute*` / `ds_permute*` / `ds_swizzle*` | `ds()` | 跨 lane 置换 | — |
| `*_rtn`（DS 原子） | `ds()` | 原子返回旧值 | — |
| `image_bvh*` | `bvh()` | 射线/节点测试，失败立刻拒 | 4857 |
| `image_atomic_*` | `image()` | UAV 原子 | 9597 |
| `image_store*` | `image()` | UAV 写 | 9591 |
| `image_sample*` / `image_gather*` | `image()` | 采样器路径 | 9551 |
| `*_cl`（图像） | `image()` | 最后一维是层数组 | 9753 |
| `*_o`（图像） | `image()` | 带 offset | 9826 |
| `buffer_load_*` / `buffer_store_*` | `buffer()` | 原始缓冲 | 10369 |
| `tbuffer_load_*` | `buffer()` | typed 缓冲 | 10371 |
| `*format*`（buffer） | `buffer()` + `buffer_format()` | typed 走格式转换 | 10333, 6049 |
| `buffer_atomic_*` | `buffer()` | 缓冲原子 | 10342 |
| `*_format_d16_*` | `buffer()` | d16 格式 | 10366 |
| `exp`（任意） | `export()` | VS/PS/GS 输出 | 10802 |
| **兜底** | `valu()` 的 `unary`/`binary` | 按名字后缀 `_f32/_i32/_u32/_b32/_f16/...` 推类型与运算 | valu_op 尾部 |

## 5 全量明细

`shader-translation-table.csv` — 1232 行，一族不落：`family,opcode,name,handler,status,note`。

```
python3 - <<'EOF'
import csv
for r in csv.DictReader(open('shader-translation-table.csv')):
    if r['status']=='refuse': print(r['family'], r['opcode'], r['name'], '|', r['note'])
EOF
```

`status` 五个值：

| status | 意思 |
|---|---|
| `explicit` | `lower.rs` 里有按**完整名字**的分支（259 条）|
| `generic` | 按族前缀/后缀统一推导，没有单独代码（395 条）|
| `impl` | 族内单独一条路径（比较、原子、采样、permute…）（468 条）|
| `nop` | 不产生宿主指令，控制流另处处理（20 条，全在 SOPP）|
| `refuse` | 返回 `Why`，没有宿主指令（90 条）|

## 6 实测：撞到拒绝的只有 5 个 shader

六轮启动日志合计 2342 条 shader 转译，成功 2337（99.79%），失败 5：

| shader | 阶段 | 偏移 | 原因 | 指令 |
|---|---|---|---|---|
| `ceb5eb68cd99cb07` | compute | `0x38` | `a descriptor's address or format read as data` | 描述符位当数字读 |
| `f133e35efed2a48a` | compute | `0x1eb4` | `a lane mask exchanged where not every lane runs` | `s_bcnt1_i32_b32` 跨半波 |
| `2fd01fca16993001` | pixel | `0x51c4` | `a descriptor read only as data` | `s_load_dwordx4` 只读不用 |
| `6b6db9f1a526daeb` | pixel | `0x49dc` | `a descriptor read only as data` | 同上 |
| `e5b2121a02f4f741` | pixel | `0x4a1c` | `a descriptor read only as data` | 同上 |

前 4 个是描述符位被当数字读：`descriptor_data()` 已按宿主视图尺寸重建 GFX10 布局
（宽度/高度/层数/记录数/格式），剩下重不出来的位（地址、format 字段本身）就在 `OTHER` 上拒。
要覆盖它们得拿到真实视图，而不是从描述符位反推 —— 那是另一件事。

第 5 个 `f133e35efed2a48a` 不是缺陷，是**模拟路线的前提**：

- 容器 `Rcascadeshadows_rendering_CS_ShadowRenderTiledCascadesAlphaCBPPPC_9945271c_Wrapped.dxbc`（69384 B），wave64 compute
- 宿主 RTX 5060 只有 32 lane（`WaveLaneCountMin == Max == 32`），console 的一次 ballot 被拆成两个 host wave
- 两半必须用 LDS 交换各自的 ballot 才能拼出 64 位（`exchange_halves`）
- 交换要求 mask 的定义块**每个 lane 都在跑**；这个 shader 在 `0x1eb4` 用了
  "对整波求 count/sum 当数据"，8 种重试形式（`without_early_outs` / `without_skips` / `local waves` × 精确/近似）
  全部撞在不同的限制上：`0x1eb4` / `0x1a34` / `0x9a84` / `0xdc70`

## 7 怎么重新生成

```bash
cd crates/victory-shader/src
# 1) opcode 表：gfx10/opcodes.rs 的 (u16, &str) 元组
# 2) 显式分支：emit/lower.rs 各 handler 函数体里的字符串字面量
# 3) 拒绝点：grep -n 'flat memory\|a scalar memory read as data\|a scalar buffer load' emit/lower.rs
```

opcode 表来自 Mesa `aco_opcodes.py`（tag mesa-22.3.0，GFX10 列），来源与本文件同步。
改了 `lower.rs` 的分派就要重跑生成脚本，否则表会撒谎。

