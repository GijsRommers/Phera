// PHP 8.5.13 explicitly retains the STDERR stream on each cloned handle.
run('copy-trace-stream', function () use ($base) {
    $stream = fopen('php://temp', 'w+');
    $original = load_url($base . '/echo', [CURLOPT_STDERR => $stream, CURLOPT_VERBOSE => true]);
    $copy = curl_copy_handle($original);
    $response = curl_exec($copy);
    return [$response, fstat($stream)['size'] > 0];
});
