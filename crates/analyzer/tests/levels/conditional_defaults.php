<?php

/** @psalm-return ($asStream is true ? resource : string) */
function readBody(bool $asStream = false)
{
    if (!$asStream) {
        return 'body';
    }

    $stream = fopen('php://temp', 'r+');
    if ($stream === false) {
        throw new RuntimeException('Cannot open stream');
    }

    return $stream;
}

function bodySize(string $encoded): int
{
    $decoded = base64_decode($encoded, true);
    if ($decoded === false) {
        return 0;
    }
    $json = json_encode($decoded);
    if ($json === false) {
        return 0;
    }
    return strlen($json) + strlen(readBody());
}
