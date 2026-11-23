$wire = file_get_contents($argv[1]);
run('gzip', fn() => gzdecode($wire, 32768));
run('auto', fn() => zlib_decode($wire, 32768));
