<?php

// Against http_client.rs's counting server: which connection answered each
// request, and with which HTTP version.

use Phera\Http\Client;

$http1 = new Client(['base_uri' => $argv[1]]);
foreach ([1, 2, 3] as $n) {
    echo 'http1 ', $http1->get("/n/$n")->body(), "\n";
}
$http2 = new Client(['base_uri' => $argv[1], 'http2_prior_knowledge' => true]);
$started = hrtime(true);
$answers = $http2->pool(array_fill(0, 4, ['uri' => '/sleep']));
$elapsed = (hrtime(true) - $started) / 1e9;
echo 'http2 ', implode(' | ', array_map(fn ($r) => $r->body(), $answers)), "\n";
echo 'multiplexed ', $elapsed < 0.6 ? 'yes' : "no ($elapsed s)", "\n";
