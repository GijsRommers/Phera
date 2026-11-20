$rows = [];
$fn = ['gzdeflate', 'gzcompress', 'gzencode'];
foreach (range(0, 95) as $index) {
    $part = match ($index % 6) {
        0 => '',
        1 => 'plain ASCII' . $index,
        2 => "\xff\0\x80" . chr($index),
        3 => chr($index) . str_repeat(chr($index * 2), 31),
        4 => "different symbols \n\r\t" . chr($index),
        5 => hash('sha256', (string) $index, true),
    };
    $data = str_repeat($part, $index % 21);
    foreach ([-15, 15, 31] as $column => $encoding) {
        foreach ([-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as $level) {
            $compress = $fn[$column];
            $rows[] = [$encoding, $level, $data, $compress($data, $level),
                       zlib_encode($data, $encoding, $level)];
        }
    }
}
foreach ([-15, 15, 31] as $encoding) {
    $data = str_repeat('large stored block.', 4000);
    $wire = zlib_encode($data, $encoding, 0);
    $rows[] = [$encoding, 0, $data, $wire, $wire];
}
file_put_contents($argv[1], serialize($rows));
