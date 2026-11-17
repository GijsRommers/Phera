<?php
// Pure computation, no waiting. The interpreter gives other requests a turn
// every few thousand loop rounds, so one long request cannot freeze the rest.
$sum = 0;
for ($i = 0; $i < 3_000_000; $i++) {
    $sum += $i % 7;
}
header('Content-Type: text/plain');
echo $sum, "\n";
