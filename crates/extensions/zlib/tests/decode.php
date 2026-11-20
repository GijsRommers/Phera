$rows = unserialize(file_get_contents($argv[1]));
foreach ($rows as $index => [$encoding, $level, $expected, $wire, $generic]) {
    $decode = match ($encoding) {
        -15 => 'gzinflate',
        15 => 'gzuncompress',
        31 => 'gzdecode',
    };
    run($index, function () use ($decode, $encoding, $wire, $expected, $level, $generic) {
        $header = match ($encoding) {-15 => 0, 15 => 2, 31 => 10};
        $trailer = match ($encoding) {-15 => 0, 15 => 4, 31 => 8};
        if ($decode($wire) !== $expected || zlib_decode($generic) !== $expected) {
            throw new RuntimeException('unexpected decompressed bytes');
        }
        return [
            substr($wire, 0, $header),
            $trailer ? substr($wire, -$trailer) : '',
            $level === 0 ? hash('sha256', $wire) : '',
            $level === 0 ? hash('sha256', $generic) : '',
        ];
    });
}
