$counter = 0;
foreach (['gzdeflate', 'gzcompress', 'gzencode', 'zlib_encode'] as $fn) {
    $generic = $fn === 'zlib_encode';
    foreach ([[], null, true, 2, 2.4, "", "\xff\0"] as $data) {
        foreach ([-99, -2, 10, PHP_INT_MAX, 'bad'] as $level) {
            run($counter++, fn() => $generic ? $fn($data, 15, $level) : $fn($data, $level));
        }
        foreach ([0, -16, 30, 32, PHP_INT_MAX, []] as $encoding) {
            run($counter++, fn() => $generic ? $fn($data, $encoding) : $fn($data, 0, $encoding));
        }
    }
    run($counter++, fn() => $fn());
}
$rows = unserialize(file_get_contents($argv[1]));
foreach (['gzinflate' => -15, 'gzuncompress' => 15, 'gzdecode' => 31, 'zlib_decode' => 31] as $fn => $encoding) {
    foreach (['', 'bad', "\0", "\xff", "\xff\xff", "\0\xfffoobar", true, [], null] as $bytes) {
        foreach ([-1, 0, 1, 'x', PHP_INT_MAX] as $length) {
            run($counter++, fn() => $fn($bytes, $length));
        }
    }
    foreach ($rows as $index => [$fmt, $level, $data, $wire]) {
        if ($fmt !== $encoding || $level !== 6 || $index % 7 !== 0 || $data === '') continue;
        foreach ([0, 1, 2, 4, strlen($wire) - 2, strlen($wire) - 1] as $cut) {
            run($counter++, fn() => $fn(substr($wire, 0, $cut)));
        }
        foreach ([1, 5, strlen($data) - 1, strlen($data), strlen($data) + 1] as $limit) {
            run($counter++, fn() => $fn($wire, $limit));
        }
        run($counter++, fn() => $fn($wire . 'ignored'));
        run($counter++, fn() => $fn(substr_replace($wire, "\xff", -1)));
    }
}
foreach ([-15, 15, 31, 0, 100] as $encoding) {
    foreach ([[], 123, ['level' => -2], ['memory' => 0], ['window' => 16],
              ['window' => 7], ['strategy' => 100], ['dictionary' => 1],
              ['dictionary' => ['']], ['dictionary' => ["a\0b"]], (object)['level' => 1]] as $opts) {
        run($counter++, fn() => deflate_init($encoding, $opts));
        run($counter++, fn() => inflate_init($encoding, $opts));
    }
}
foreach ([-15, 15, 31] as $encoding) {
    $text = str_repeat("content \0 to stream\n", 53);
    foreach ([ZLIB_SYNC_FLUSH, ZLIB_FULL_FLUSH, ZLIB_PARTIAL_FLUSH, ZLIB_NO_FLUSH] as $flush) {
        $encoder = deflate_init($encoding);
        $wire = deflate_add($encoder, substr($text, 0, 137), $flush)
                . deflate_add($encoder, substr($text, 137), ZLIB_FINISH);
        $decoder = inflate_init($encoding);
        run($counter++, function () use ($decoder, $wire, $text) {
            $chunks = str_split($wire, 3);
            $out = '';
            foreach ($chunks as $chunk) $out .= inflate_add($decoder, $chunk);
            return [$out === $text, inflate_get_status($decoder),
                inflate_get_read_len($decoder) === strlen($wire)];
        });
    }
    foreach (['phrase repeated in dictionary', ["phrase", "repeated", "dictionary"]] as $dictionary) {
        if ($encoding === 31) continue;
        $encoder = deflate_init($encoding, ['dictionary' => $dictionary]);
        $decoder = inflate_init($encoding, ['dictionary' => $dictionary]);
        $wire = deflate_add($encoder, str_repeat('phrase repeated in dictionary', 30), ZLIB_FINISH);
        run($counter++, fn() => inflate_add($decoder, $wire, ZLIB_FINISH));
    }
    $ctx = inflate_init($encoding);
    run($counter++, fn() => [inflate_get_status($ctx), inflate_get_read_len($ctx)]);
    run($counter++, fn() => inflate_add($ctx, '', 9));
    run($counter++, fn() => deflate_add(deflate_init($encoding), '', 9));
    run($counter++, fn() => serialize($ctx));
    run($counter++, fn() => clone $ctx);
    run($counter++, fn() => new InflateContext());
    run($counter++, fn() => new DeflateContext());
}
run($counter++, fn() => ob_gzhandler('hello', PHP_OUTPUT_HANDLER_START | PHP_OUTPUT_HANDLER_FINAL));
run($counter++, fn() => zlib_get_coding_type());
run($counter++, fn() => [ini_get('zlib.output_compression'),
    ini_set('zlib.output_compression', '0'), ini_get('zlib.output_compression_level')]);

$path = 'example.gz';
run($counter++, fn() => gzopen('missing.gz', 'rb'));
run($counter++, fn() => gzopen("a\0b", "r"));
run($counter++, fn() => gzopen('example.gz', 'r+'));
$fp = gzopen($path, 'wb2');
run($counter++, fn() => gzwrite($fp, "one\ntwo\n"));
run($counter++, fn() => gzputs($fp, "three!oops", 6));
run($counter++, fn() => gztell($fp));
run($counter++, fn() => gzclose($fp));
$fp = gzopen($path, 'ab');
run($counter++, fn() => gzwrite($fp, "four\n"));
run($counter++, fn() => gzclose($fp));
$fp = gzopen($path, 'rb');
run($counter++, fn() => gzgets($fp));
run($counter++, fn() => gzread($fp, 3));
run($counter++, fn() => gzgetc($fp));
run($counter++, fn() => gztell($fp));
run($counter++, fn() => gzseek($fp, 4));
run($counter++, fn() => gzeof($fp));
run($counter++, fn() => gzrewind($fp));
run($counter++, fn() => gzpassthru($fp));
run($counter++, fn() => gzeof($fp));
run($counter++, fn() => gzclose($fp));
run($counter++, fn() => gzfile($path));
run($counter++, fn() => readgzfile($path));
run($counter++, fn() => file_get_contents('compress.zlib://' . $path));
run($counter++, fn() => in_array('compress.zlib', stream_get_wrappers(), true));
run($counter++, fn() => file_put_contents('compress.zlib://wrapper.gz', "wrapped\n"));
run($counter++, fn() => gzfile('wrapper.gz'));
file_put_contents('plain.txt', "not gzipped\n");
run($counter++, fn() => gzfile('plain.txt'));
