function loopback_tls($tls, $options) {
    return load_url($tls . '/identity', $options + [
        CURLOPT_RESOLVE => ['localhost:' . parse_url($tls, PHP_URL_PORT) . ':127.0.0.1'],
    ]);
}
run('tls-capath', fn() => transfer_result(loopback_tls($tls, [CURLOPT_CAPATH => $tmp . '/capath'])));
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
