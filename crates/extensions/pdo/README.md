# PDO connections in Phera

Phera supports PostgreSQL and MySQL through PHP's PDO API. Connections are pooled
by default across the entire process, including all `phera serve --workers N` workers.
A pool is shared by connections with the same driver, DSN session options and credentials.

```php
$pdo = new PDO($dsn, $user, $password);
$value = $pdo->query('SELECT 1')->fetchColumn();
```

A PDO object exclusively borrows one connection. Its statements keep that connection
borrowed even after the PDO variable is freed. After the last handle is freed, Phera
rolls back any open transaction, removes temporary tables, session variables and prepared
statements, and returns the clean connection to the pool. Cleanup runs in the background.
Cancelled requests follow the same cleanup path; an in-flight query finishes before reset.

When every connection is busy, constructors wait in arrival order. A timeout throws a
PDOException whose message says the pool is exhausted and includes the connection limit.
Idle or old connections are closed, and stale idle connections are checked before reuse.

## Application settings

Set these environment variables before starting Phera. They are read once, on first PDO use.
Values are positive integers; durations are in seconds. Settings apply to every pool.

| Variable | Default | Meaning |
| --- | ---: | --- |
| `PHERA_PDO_POOL_SIZE` | 20 | Maximum connections per pool across all workers |
| `PHERA_PDO_POOL_IDLE_SECONDS` | 60 | Maximum time a returned connection stays idle |
| `PHERA_PDO_POOL_LIFETIME_SECONDS` | 1800 | Maximum age; borrowed connections retire on return |
| `PHERA_PDO_POOL_WAIT_SECONDS` | 30 | Maximum time to acquire a pooled connection |
| `PHERA_PDO_IO_THREADS` | 2 | Threads in Phera's dedicated database I/O runtime |

A different database, user, password or DSN session setting gets a different pool.
`PDO::ATTR_PERSISTENT` is accepted and does not change pooling. Scripts cannot set pool
sizes or timeouts. To opt a connection out:

```php
$pdo = new PDO($dsn, $user, $password, [PDO::PHERA_ATTR_POOL => false]);
```

An opted-out connection still uses asynchronous database I/O, but is closed on final
release and does not count against a pool's limit. PHP values stay on their worker;
connecting, queries, reset and close run on the database I/O threads.

PostgreSQL cleanup uses `ROLLBACK` followed by `DISCARD ALL`. MySQL uses the driver's
reset-connection command (change-user on older servers), then restores the DSN database
and charset. A failed reset discards the connection. Pooling does not make unsupported
PDO features available: custom-class fetching and bytea streams still need runtime support.

PostgreSQL connections that prepared custom types, or failed while preparing, are conservatively
closed on return: tokio-postgres keeps private type-info statements that cannot be cleared through
its public API after `DISCARD ALL`. This avoids lending stale driver metadata to another script.
