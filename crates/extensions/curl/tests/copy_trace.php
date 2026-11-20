// PHP 8.5.13 explicitly retains the STDERR stream on each cloned handle.
run('copy-trace-stream', function () use ($base, $tmp) {
    $path = $tmp . '/copy-trace.log';
    $stream = fopen($path, 'w+b');
    $original = load_url($base . '/echo', [CURLOPT_STDERR => $stream, CURLOPT_VERBOSE => true]);
    $copy = curl_copy_handle($original);
    $response = curl_exec($copy);
    fflush($stream);
    rewind($stream);
    return [$response, strlen(stream_get_contents($stream)) > 0];
});
