# Local patches

Copy of tiny_http 0.12.0 with two changes, used through `[patch.crates-io]`:

- `src/util/task_pool.rs`: connections that arrive together no longer get stuck in the queue
  (the pool counted a thread that had already been woken as still waiting).
- `src/connection.rs`: `TCP_NODELAY` on accepted sockets, so keep-alive requests are answered
  without the ~40 ms delay.
