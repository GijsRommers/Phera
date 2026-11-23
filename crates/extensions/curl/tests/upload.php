run('upload-callback-sized', function () use ($base) {
    $length = 170003;
    $sent = 0;
    $handle = load_url($base . '/upload-size', [
        CURLOPT_UPLOAD => true, CURLOPT_INFILESIZE => $length,
        CURLOPT_READFUNCTION => function ($curl, $input, $limit) use (&$sent, $length) {
            $count = min($limit, $length - $sent, 8192);
            $sent += $count;
            return str_repeat('c', $count);
        },
    ]);
    return [curl_exec($handle), $sent, curl_getinfo($handle, CURLINFO_SIZE_UPLOAD_T)];
});
run('upload-callback-unknown-length', function () use ($base) {
    $calls = 0;
    $handle = load_url($base . '/upload-size', [
        CURLOPT_UPLOAD => true,
        CURLOPT_HTTPHEADER => ['Expect:'],
        CURLOPT_READFUNCTION => function () use (&$calls) {
            return $calls++ < 4 ? str_repeat('u', 8192) : '';
        },
    ]);
    return [curl_exec($handle), curl_errno($handle), $calls];
});
run('upload-infile-replayed', function () use ($base, $tmp) {
    $path = $tmp . '/replay.bin';
    $length = 170003;
    file_put_contents($path, str_repeat('f', $length));
    $input = fopen($path, 'rb');
    $handle = load_url($base . '/upload-redirect', [
        CURLOPT_UPLOAD => true, CURLOPT_INFILESIZE => $length, CURLOPT_INFILE => $input,
        CURLOPT_FOLLOWLOCATION => true, CURLOPT_HTTPHEADER => ['Expect:'],
    ]);
    $result = [curl_exec($handle), curl_errno($handle), curl_getinfo($handle, CURLINFO_REDIRECT_COUNT)];
    fclose($input);
    return $result;
});
run('upload-callback-abort', function () use ($base) {
    $handle = load_url($base . '/upload-size', [
        CURLOPT_UPLOAD => true, CURLOPT_INFILESIZE => 10,
        CURLOPT_READFUNCTION => fn() => 268435456,
    ]);
    return [curl_exec($handle), curl_errno($handle), curl_error($handle)];
});
run('upload-callback-short-read', function () use ($base) {
    $calls = 0;
    $handle = load_url($base . '/upload-size', [
        CURLOPT_UPLOAD => true, CURLOPT_INFILESIZE => 100,
        CURLOPT_READFUNCTION => function () use (&$calls) {
            return $calls++ === 0 ? 'short' : '';
        },
    ]);
    return [curl_exec($handle), curl_errno($handle), $calls, curl_error($handle)];
});
