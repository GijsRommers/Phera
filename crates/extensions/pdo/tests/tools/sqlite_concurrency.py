"""Compare 200 concurrent WAL requests with PHP; prove a slow query yields the PHP worker."""
import concurrent.futures
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

phera, php = sys.argv[1:]


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def request(address, path='index.php'):
    try:
        with urllib.request.urlopen(f'http://{address}/{path}', timeout=45) as response:
            return response.status, response.read().decode()
    except Exception as error:
        return 500, str(error)


def concurrent_request(address, barrier):
    barrier.wait(timeout=30)
    return request(address)


def serve(root, kind, workers):
    address = f'127.0.0.1:{port()}'
    command = ([phera, 'serve', '--listen', address, '--workers', str(workers), str(root)]
               if kind == 'phera' else [php, '-n', '-S', address, '-t', str(root)])
    env = dict(os.environ, PHP_CLI_SERVER_WORKERS=str(workers))
    log = open(root / f'{kind}.log', 'wb')
    process = subprocess.Popen(
        command, env=env, stdout=log, stderr=log, start_new_session=True)
    for _ in range(200):
        if process.poll() is not None:
            raise AssertionError((root / f'{kind}.log').read_text())
        try:
            with socket.create_connection(address.split(':')[:1] + [int(address.split(':')[1])], .1):
                return process, address, log
        except OSError:
            time.sleep(.025)
    raise AssertionError('server did not start')


def stop(process, log):
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()
    log.close()


with tempfile.TemporaryDirectory(prefix='phera-sqlite-concurrency-') as temporary:
    root = Path(temporary)
    outcomes = {}
    for kind in ['php', 'phera']:
        directory = root / kind
        directory.mkdir()
        database = str(directory / 'shared.db').replace("'", "''")
        init = directory / 'init.php'
        init.write_text("<?php $p=new PDO('sqlite:" + database + "');"
                        "$p->exec('PRAGMA journal_mode=WAL;"
                        "CREATE TABLE a(id INTEGER PRIMARY KEY,v)');")
        run = subprocess.run([php, '-n', str(init)], capture_output=True)
        assert run.returncode == 0, run.stdout
        (directory / 'index.php').write_text("<?php $p=new PDO('sqlite:" + database + "');"
            "$p->setAttribute(PDO::ATTR_TIMEOUT,10);"
            "$p->exec('INSERT INTO a(v) VALUES(42)');"
            "$s=$p->prepare('SELECT v FROM a WHERE id=?');"
            "$s->execute([$p->lastInsertId()]);echo $s->fetchColumn();")
        process, address, log = serve(directory, kind, 2)
        try:
            started = time.monotonic()
            with concurrent.futures.ThreadPoolExecutor(max_workers=200) as executor:
                barrier = threading.Barrier(200)
                futures = [executor.submit(concurrent_request, address, barrier)
                           for _ in range(200)]
                results = [future.result() for future in futures]
            failures = [result for result in results if result != (200, '42')]
            outcomes[kind] = dict(requests=200, failures=len(failures),
                                  seconds=time.monotonic()-started)
            assert not failures, failures[:5]
            verify = subprocess.run([php, '-n', '-r',
                "$p=new PDO('sqlite:"+database+"');"
                "echo $p->query('SELECT count(*) FROM a')->fetchColumn();"],
                capture_output=True)
            assert verify.stdout == b'200', verify.stdout
        finally:
            stop(process, log)
    assert outcomes['phera']['failures'] <= outcomes['php']['failures']
    # A one-worker server ensures the quick and slow requests share the worker.
    (root / 'slow.php').write_text("<?php $p=new PDO('sqlite::memory:');"
        "$p->query('WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n "
        "WHERE x<5000000) SELECT sum(x) FROM n');echo 'slow';")
    (root / 'quick.php').write_text("<?php $p=new PDO('sqlite::memory:');"
                                  "echo $p->query('SELECT 42')->fetchColumn();")
    process, address, log = serve(root, 'phera', 1)
    try:
        assert request(address, 'quick.php') == (200, '42')
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            started = time.monotonic()
            slow = executor.submit(request, address, 'slow.php')
            time.sleep(.1)
            quick_start = time.monotonic()
            assert request(address, 'quick.php') == (200, '42')
            quick_elapsed = time.monotonic()-quick_start
            assert not slow.done(), 'quick request waited for slow query'
            assert quick_elapsed < .5, quick_elapsed
            assert slow.result() == (200, 'slow')
            outcomes['same_worker'] = dict(quick_seconds=quick_elapsed,
                                           slow_seconds=time.monotonic()-started)
    finally:
        stop(process, log)
    print(json.dumps(outcomes, indent=2))
