<?php

[$script, $protocol, $pem] = $argv;
$key = openssl_pkey_new(['private_key_bits' => 2048]);
$csr = openssl_csr_new(['commonName' => 'localhost'], $key, ['digest_alg' => 'sha256']);
$certificate = openssl_csr_sign($csr, null, $key, 1, ['digest_alg' => 'sha256']);
openssl_x509_export($certificate, $certText);
openssl_pkey_export($key, $keyText);
file_put_contents($pem, $certText . $keyText);
$context = stream_context_create(['ssl' => [
    'local_cert' => $pem,
    'ciphers' => 'DEFAULT:@SECLEVEL=0',
    'security_level' => 0,
]]);
$server = stream_socket_server('tcp://127.0.0.1:0', context: $context);
echo stream_socket_get_name($server, false), "\n";
while ($socket = @stream_socket_accept($server, 10)) {
    stream_set_timeout($socket, 3);
    if ($protocol === 'pgsql') {
        if (stream_get_contents($socket, 8) !== hex2bin('0000000804d2162f')) {
            fclose($socket);
            continue;
        }
        fwrite($socket, 'S');
    }
    if (@stream_socket_enable_crypto($socket, true, STREAM_CRYPTO_METHOD_TLSv1_0_SERVER)) {
        fwrite($socket, stream_get_meta_data($socket)['crypto']['protocol']);
    }
    fclose($socket);
}
