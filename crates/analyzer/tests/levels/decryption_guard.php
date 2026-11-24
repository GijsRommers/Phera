<?php
declare(strict_types=1);

function decryptText(string $data, string $nonce, string $key): string
{
    $plaintext = sodium_crypto_secretbox_open($data, $nonce, $key);
    if ($plaintext === false) {
        throw new RuntimeException();
    }
    return $plaintext;
}
