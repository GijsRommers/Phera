<?php

// Against http_client.rs's TLS server, whose certificate is self-signed.

use Phera\Http\{Client, ClientException};

echo 'unverified h2: ', (new Client(['verify' => false]))->get($argv[1] . '/tls')->body(), "\n";
echo 'unverified h1: ', (new Client(['verify' => false, 'http2' => false]))->get($argv[1] . '/tls')->body(), "\n";
try {
    (new Client())->get($argv[1] . '/tls');
    echo "verified: accepted\n";
} catch (ClientException $e) {
    echo 'verified: ', str_contains($e->getMessage(), 'certificate') ? 'refused' : $e->getMessage(), "\n";
}
