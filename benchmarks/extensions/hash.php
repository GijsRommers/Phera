<?php
// Same script on PHP and Phera; measure only the work inside each process.
foreach ([16 => 1000000, 1048576 => 100] as $length => $count) {
    $data = str_repeat('x', $length);
    foreach (['md5', 'sha1', 'sha256', 'xxh3', 'crc32'] as $algorithm) {
        $start = hrtime(true);
        if ($algorithm === 'sha256' || $algorithm === 'xxh3') {
            for ($i = 0; $i < $count; $i++) { $result = hash($algorithm, $data); }
        } else {
            for ($i = 0; $i < $count; $i++) { $result = $algorithm($data); }
        }
        $elapsed = (hrtime(true) - $start) / 1000000000;
        echo $algorithm, ' ', $length, ' ', $count, ' ', $elapsed, ' ', $result, "\n";
    }
}
