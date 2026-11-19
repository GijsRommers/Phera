<?php
// Exercise the Laravel encrypted-cookie payload format with two runtimes.
[$script, $operation, $cipher, $payload] = array_pad($argv, 4, '');
if (!in_array($cipher, ['aes-256-cbc', 'aes-256-gcm'], true)) {
    exit(1);
}

$key = str_repeat("\x91", 32);
$plaintext = "Phera Laravel cookie \0\xff";
$authenticated = $cipher === 'aes-256-gcm';

if ($operation === 'encrypt') {
    $iv = random_bytes(openssl_cipher_iv_length($cipher));
    $value = openssl_encrypt(serialize($plaintext), $cipher, $key, 0, $iv, $tag);
    if ($value === false) {
        exit(2);
    }

    $iv = base64_encode($iv);
    $tag = base64_encode($tag ?? '');
    $mac = $authenticated ? '' : hash_hmac('sha256', $iv . $value, $key);
    echo base64_encode(json_encode(
        ['iv' => $iv, 'value' => $value, 'mac' => $mac, 'tag' => $tag],
        JSON_UNESCAPED_SLASHES,
    ));
} elseif ($operation === 'decrypt') {
    $decoded = json_decode(base64_decode($payload), true);
    $iv = base64_decode($decoded['iv'], true);
    $tag = base64_decode($decoded['tag'], true);
    if (strlen($iv) !== openssl_cipher_iv_length($cipher)) {
        exit(3);
    }
    if ($authenticated) {
        if (strlen($tag) !== 16) {
            exit(4);
        }
    } elseif ($tag !== '' || !hash_equals(
        hash_hmac('sha256', $decoded['iv'] . $decoded['value'], $key),
        $decoded['mac'],
    )) {
        exit(5);
    }

    $value = openssl_decrypt($decoded['value'], $cipher, $key, 0, $iv, $tag);
    if ($value === false || unserialize($value) !== $plaintext) {
        exit(6);
    }
    echo "verified\n";
} else {
    exit(7);
}
