# Breakthrough 6: Multi-worker PDO pool fairness, bounded VM concurrency, and zero-copy fork metadata

Date: 2026-11-23

## Context

Under a strict 2-core / 2 GiB RAM PostgreSQL 17.11 load test (`systemd-run --user --scope -p CPUQuota=200% -p MemoryMax=2G -- taskset -c 0,1`, `--workers 2`, keep-alive load generator on separate cores, PostgreSQL on separate cores), Breakthrough 5 (`fbebc25ad`) beat PHP 8.5.13 FPM + nginx on Symfony `/api/products` L2 at `c=16` (`1,002.6` vs `821.0` RPS), but uncovered three concrete concurrency and memory bottlenecks at `c=64` and `c=128`:

1. **Multi-worker PDO pool head-of-line connection teardown thrash**:
   - `Admission::offer(worker, idle)` in `crates/db-drivers/src/pool/admission.rs` only inspected `state.waiters.front()`. Whenever the front waiter belonged to another worker (`W0` vs `W1`), `offer()` returned `Some(idle)` without checking for a matching waiter on `worker`, and `Worker::recycle()` immediately called `Admission::reclaim()` and closed the freshly-reset PostgreSQL TCP connection.
   - At `c=64` and `c=128` on `/api/products-io`, this tore down and re-authenticated ~3,400 PostgreSQL TCP + SCRAM-SHA-256 sessions every 10 seconds (~545 new sessions per 1,000 requests), dropping `/api/products` L2 from `1,002.6` (`c=16`) to `863.6` RPS (`c=64`).
   - Even after fixing `offer()` to hand off to `worker`'s next matching waiter (`ef9d0aa88`), a worker momentarily idle for `<1 ms` between sequential requests still triggered `reclaim()` whenever the peer worker had a waiter, causing a `31↔33` ping-pong (`1,723` sessions in 10s).
   - Default `PHERA_PDO_POOL_SIZE` of `20` also capped 20 ms I/O-bound concurrency below PHP-FPM's 40 static children (`637` vs `721` RPS).

2. **Unbounded concurrent `fork_vm()` memory growth**:
   - `WebWorker::handle_web_request` immediately forked a full VM for every accepted HTTP request with no per-worker active VM cap. At `c=128`, 128 simultaneous forked VMs pushed cgroup `memory.peak` to `448.5 MiB` (steady RSS `312.8 MiB`).

3. **Redundant per-fork allocations and SipHash tables in `Vm::fork_vm`**:
   - `Registers::fork_copy` copied the entire unused `None` tail capacity of `self.slots` past `self.top`.
   - `IncludeState`, `FunctionTable`, and `ConstantTable` deep-cloned hundreds of heap-allocated file/function/constant `String` keys on every fork.
   - `Globals`, `StaticVariables`, `DynamicVariables`, `ConstantTable`, `FunctionTable`, `IncludeState`, `unset_typed_properties`, `prelinked_classes`, and `extension_values` still used `std`'s SipHash instead of `foldhash`.

## Changes

1. **Fair-share multi-worker PDO pool with 25 ms idle grace (`crates/db-drivers/src/pool/{admission,worker}.rs`, `crates/db-drivers/src/pool.rs`)**:
   - Track per-worker open socket counts (`open_by_worker`) inside `Admission::State` and bind each `Permit` to its owning `WorkerId`.
   - In `Admission::offer`, preserve strict FIFO across distinct `Key`s and whenever `limit < active_workers` or `open_of(worker) > fair_share && open_of(front.worker) < fair_share`. In balanced steady state, hand the reset socket directly to `worker`'s next matching waiter with zero teardown.
   - Add `Admission::reclaim_on_return` and a `25 ms` `RECLAIM_GRACE` in `Worker::maintain` so balanced workers hold returned sockets across sub-millisecond inter-request gaps without ping-ponging, while genuinely idle workers release sockets after 25 ms and over-subscribed workers yield immediately.
   - Raise default `PHERA_PDO_POOL_SIZE` from `20` to `64`.

2. **Bounded concurrent active VMs per worker (`crates/runtime/src/lib.rs`)**:
   - Add a per-worker `Rc<tokio::sync::Semaphore>` defaulting to `32` concurrent active VMs per worker (`PHERA_MAX_CONCURRENT_REQUESTS_PER_WORKER`, `0` for unbounded, also configurable via `Runtime::with_max_concurrent_requests_per_worker`).
   - Acquire the permit immediately before `snapshot.fork()` / `new_classic_vm()` and explicitly drop `vm` before releasing the permit so freed memory is available before the next parked request forks.

3. **Zero-copy fork metadata & `foldhash` (`crates/vm/src/`)**:
   - `Registers::fork_copy` copies only the live prefix `self.slots[..self.top]`.
   - `IncludeState` splits into `base: Rc<IndexSet<String, RandomState>>` + `added`; snapshot creation flattens `added` into `base` once, making every subsequent fork an O(1) `Rc::clone` without copying even when L1 requests include additional files.
   - `FunctionTable` (`functions`, `order`) and `ConstantTable` (`user_order`, `origins`) share snapshot tables copy-on-write via `Rc::make_mut`.
   - Switch remaining VM lookup tables (`Globals`, `StaticVariables`, `DynamicVariables`, `ConstantTable`, `FunctionTable`, `IncludeState`, `unset_typed_properties`, `prelinked_classes`, `extension_values`) to `foldhash`.

## Verified Linux Results (Hades, fat LTO, PHP 8.5.13 FPM OPcache ON / JIT OFF)

### 2-Core / 2 GiB PostgreSQL 17.11 Throughput (`--workers 2`, default env)

All measured HTTP responses returned HTTP 200 and matched PHP 8.5.13 FPM byte-for-byte. The live PostgreSQL PDO pool contract passed.

| Route | Mode | c | BT6-v2 RPS | BT6-v1 RPS | BT5 RPS | PHP-FPM + nginx | Δ vs BT5 | v2 PG sessions (10s) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Symfony `/api/products` | L2 | 16 | **1,074.5** | 1,087.4 | 1,002.6 | 821.0 | **+7.2%** (+30.9% vs FPM) | **0** |
| Symfony `/api/products` | L2 | 64 | **1,051.1** | 1,004.0 | 863.6 | 853.7 | **+21.7%** (+23.1% vs FPM) | **2** |
| Symfony `/api/products-io` (20ms DB) | L2 | 64 | **909.9** | 914.5 | 637.3 | 721.0 | **+42.8%** (+26.2% vs FPM) | **55** (down from 3,458, −98.4%) |
| Symfony `/api/products-io` (20ms DB) | L2 | 128 | **791.9** | 786.2 | 614.2 | 690.1 | **+28.9%** (+14.8% vs FPM) | **14** (down from 3,359, −99.6%) |
| Symfony `/api/ping` (plain JSON) | L2 | 16 | — | **1,513.0** | — | 1,938.0 | — | 0 |
| Symfony `/api/ping` (plain JSON) | L2 | 64 | — | **1,441.4** | — | 1,984.3 | — | 0 |
| Laravel `/api/posts?page=2` | L2 | 16 | **242.9** | 248.8 | 240.2 | 493.5 | **+1.1%** | **8** |
| Laravel `/api/posts?page=2` | L2 | 64 | **237.3** (mean of 238.3 / 236.3) | 232.4 | 218.3 | 512.7 | **+8.7%** | **4–7** |

- **Memory under concurrency (`/api/products-io` L2)**:
  - `c=64`: cgroup `memory.peak` dropped **357.0 → 288.1 MiB** (**−19.3%**).
  - `c=128`: cgroup `memory.peak` dropped **448.5 → 324.1 MiB** (**−27.7%**), steady RSS **312.8 → 245.2 MiB** (**−21.6%**).

### Paired Sequential `perf stat` (Two Interleaved Rounds, 1 Worker, 200 Req/Run)

All 8/8 HTTP workloads reduced instructions in both rounds (100% SHA-256 parity); all 23/23 CLI benchmarks matched PHP 8.5.13 CLI stdout (19/23 cycle wins, 18/23 instruction wins, largest instruction delta +0.07% on `calls`):

| HTTP Workload | BT6 ins/req | BT5 ins/req | PHP FPM ins/req | Δ ins vs BT5 | BT6 / PHP |
| --- | ---: | ---: | ---: | ---: | ---: |
| Symfony `/api/products` L1 | 28.19M | 28.49M | 6.02M | −1.07% | 4.68× |
| Symfony `/api/products` L2 | 7.46M | 7.63M | 6.02M | −2.14% | **1.24×** |
| Symfony `/api/ping` L1 | 12.31M | 12.51M | 3.21M | −1.60% | 3.84× |
| Symfony `/api/ping` L2 | 5.77M | 5.88M | 3.21M | −1.85% | **1.80×** |
| Laravel `/api/hello` L1 | 20.07M | 20.36M | 5.96M | −1.43% | 3.37× |
| Laravel `/api/hello` L2 | 15.68M | 15.87M | 5.96M | −1.18% | 2.63× |
| Laravel `/api/posts?page=2` L1 | 46.92M | 47.36M | 13.70M | −0.92% | 3.42× |
| Laravel `/api/posts?page=2` L2 | 34.14M | 34.39M | 13.70M | −0.72% | **2.49×** |

## Trade-offs and Next Steps

- On Linux `SO_REUSEPORT` with 2 workers at `c=64`, uneven keep-alive socket hashing (observed 37/27 vs 33/31 splits on Laravel `/api/posts?page=2`) contributes to p99 tail variance when one worker queues above its 32-VM cap.
- Laravel `/api/posts?page=2` L2 remains `2.49×` PHP instructions (`Machine::run` 17.66% self, `Vm::fork_vm` 4.53% self, `drop_glue::<Value>` 2.30%, `Machine::call_method` 2.26%). Next structural wins: copy-on-write snapshot objects/properties, compiler last-use temp move semantics, and fused bytecode superinstructions.
