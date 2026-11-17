<?php
function run() {
    $sum = 0;
    for ($i = 0; $i < 5000000; $i++) {
        $sum += $i % 7;
        if ($i % 3 == 0) { $sum -= 1; }
    }
    return $sum;
}
echo run(), "\n";
