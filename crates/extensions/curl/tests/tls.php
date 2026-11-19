function loopback_tls($tls, $options) {
    return load_url($tls . '/identity', $options + [
        CURLOPT_RESOLVE => ['localhost:' . parse_url($tls, PHP_URL_PORT) . ':127.0.0.1'],
    ]);
}
run('tls-capath', fn() => transfer_result(loopback_tls($tls, [CURLOPT_CAPATH => $tmp . '/capath'])));
run('tls-default-protocol-after-url', fn() => transfer_result(load_url(substr($tls, 8) . '/identity', [
    CURLOPT_DEFAULT_PROTOCOL => 'https', CURLOPT_CAINFO => $ca,
    CURLOPT_RESOLVE => ['localhost:' . parse_url($tls, PHP_URL_PORT) . ':127.0.0.1'],
])));
run('tls-pinned', fn() => transfer_result(loopback_tls($tls, [
    CURLOPT_CAINFO_BLOB => file_get_contents($ca),
    CURLOPT_PINNEDPUBLICKEY => 'sha256//AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=;' . $pin,
])));
run('tls-pin-mismatch', fn() => transfer_result(loopback_tls($tls, [
    CURLOPT_CAINFO => $ca,
    CURLOPT_PINNEDPUBLICKEY => 'sha256//AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',
])));
run('tls-pin-still-verifies-trust', function () use ($tls, $pin) {
    $handle = loopback_tls($tls, [CURLOPT_PINNEDPUBLICKEY => $pin]);
    return [curl_exec($handle), curl_errno($handle)];
});
run('tls-client-pem-encrypted', fn() => transfer_result(loopback_tls($tls, [
    CURLOPT_CAINFO => $ca, CURLOPT_SSLCERT => $tmp . '/client.pem', CURLOPT_SSLCERTTYPE => 'PEM',
    CURLOPT_SSLKEY => $tmp . '/client-key.pem', CURLOPT_SSLKEYTYPE => 'PEM',
    CURLOPT_KEYPASSWD => 'secret',
])));
run('tls-client-der-blob', fn() => transfer_result(loopback_tls($tls, [
    CURLOPT_CAINFO => $ca, CURLOPT_SSLCERT_BLOB => file_get_contents($tmp . '/client.der'),
    CURLOPT_SSLCERTTYPE => 'DER', CURLOPT_SSLKEY_BLOB => file_get_contents($tmp . '/client-key.der'),
    CURLOPT_SSLKEYTYPE => 'DER',
])));
function verify_result($handle) {
    return [curl_exec($handle), curl_errno($handle)];
}
$ipTls = str_replace('localhost', '127.0.0.1', $tls);
run('tls-peer-disabled', fn() => verify_result(loopback_tls($tls, [
    CURLOPT_SSL_VERIFYPEER => false, CURLOPT_CAINFO => $tmp . '/missing.pem',
])));
run('tls-peer-disabled-still-checks-host', fn() => verify_result(loopback_tls($ipTls, [
    CURLOPT_SSL_VERIFYPEER => false,
])));
run('tls-host-disabled', fn() => verify_result(loopback_tls($ipTls, [
    CURLOPT_SSL_VERIFYHOST => 0, CURLOPT_CAINFO => $ca,
])));
run('tls-host-disabled-still-checks-peer', fn() => verify_result(loopback_tls($ipTls, [
    CURLOPT_SSL_VERIFYHOST => 0,
])));
run('tls-both-disabled-still-check-pins', fn() => verify_result(loopback_tls($ipTls, [
    CURLOPT_SSL_VERIFYPEER => false, CURLOPT_SSL_VERIFYHOST => 0,
    CURLOPT_PINNEDPUBLICKEY => 'sha256//AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',
])));
run('tls-both-disabled-then-reset', function () use ($ipTls) {
    $handle = loopback_tls($ipTls, [CURLOPT_SSL_VERIFYPEER => false, CURLOPT_SSL_VERIFYHOST => 0]);
    $first = verify_result($handle);
    curl_reset($handle);
    curl_setopt_array($handle, [
        CURLOPT_URL => $ipTls . '/identity', CURLOPT_RETURNTRANSFER => true, CURLOPT_PROXY => '',
    ]);
    return [$first, verify_result($handle)];
});
run('tls-peer-disabled-trace', function () use ($tls, $tmp) {
    $file = fopen($tmp . '/verification-trace', 'w+b');
    $handle = loopback_tls($tls, [
        CURLOPT_VERBOSE => true, CURLOPT_STDERR => $file, CURLOPT_SSL_VERIFYPEER => false,
    ]);
    $result = verify_result($handle);
    fflush($file);
    rewind($file);
    $trace = stream_get_contents($file);
    fclose($file);
    return [$result, str_contains($trace, 'peer verification disabled')
        || str_contains($trace, 'certificate verify result: self-signed certificate')
        || str_contains($trace, 'certificate verification failed, continuing anyway')];
});
