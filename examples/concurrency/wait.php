<?php
// Stands in for a request that waits on something outside PHP: a database
// query, a call to another service. It waits 100 ms and answers.
$start = hrtime(true);
usleep(100_000);
$waited = (hrtime(true) - $start) / 1_000_000;
header('Content-Type: text/plain');
echo "waited ", round($waited), " ms\n";
