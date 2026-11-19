function load_url($url, $options = []) {
    $handle = curl_init($url);
    curl_setopt_array($handle, $options + [CURLOPT_RETURNTRANSFER => true, CURLOPT_PROXY => '']);
    return $handle;
}
function transfer_result($handle) {
    $body = curl_exec($handle);
    $info = curl_getinfo($handle);
    return [$body, $info['http_code'], $info['content_type'], $info['http_version'],
        $info['effective_method'], $info['redirect_count'], $info['scheme'],
        curl_errno($handle), curl_error($handle)];
}
run('get', fn() => transfer_result(load_url($base . '/echo')));
run('post', fn() => transfer_result(load_url($base . '/echo', [
    CURLOPT_POSTFIELDS => "{\"hello\":\"world\"}",
    CURLOPT_HTTPHEADER => ['Content-Type: application/json', 'X-Test: accepted'],
])));
run('head', fn() => transfer_result(load_url($base . '/echo', [CURLOPT_NOBODY => true])));
run('redirect', fn() => transfer_result(load_url($base . '/redirect', [
    CURLOPT_FOLLOWLOCATION => true, CURLOPT_AUTOREFERER => true, CURLOPT_POSTFIELDS => 'change',
])));
run('preserve-post', fn() => transfer_result(load_url($base . '/redirect-307', [
    CURLOPT_FOLLOWLOCATION => true, CURLOPT_POSTFIELDS => 'keep',
])));
run('redirect-limit', fn() => transfer_result(load_url($base . '/redirect', [
    CURLOPT_FOLLOWLOCATION => true, CURLOPT_MAXREDIRS => 1,
])));
run('gzip', fn() => transfer_result(load_url($base . '/gzip', [
    CURLOPT_ACCEPT_ENCODING => 'gzip',
])));
run('chunks', fn() => transfer_result(load_url($base . '/chunks')));
run('cut', function () use ($base) {
    $result = transfer_result(load_url($base . '/cut'));
    // Older libcurl releases use another phrase for the same byte count.
    $result[8] = preg_replace('/^transfer closed with (\d+) bytes remaining to read$/',
        'end of response with $1 bytes missing', $result[8]);
    return $result;
});
run('timeout', function () use ($base) {
    $handle = load_url($base . '/wait', [CURLOPT_TIMEOUT_MS => 40]);
    $result = curl_exec($handle);
    $error = curl_error($handle);
    $valid = preg_match('/^Operation timed out after (\d+) milliseconds with 0 bytes received$/',
        $error, $parts);
    return [$result, curl_errno($handle),
        $valid && (int) $parts[1] >= 40 && (int) $parts[1] < 120,
        preg_replace('/after \d+ milliseconds/', 'after {elapsed} milliseconds', $error)];

});
run('http-failure', fn() => transfer_result(load_url($base . '/fail', [
    CURLOPT_FAILONERROR => true,
])));
run('limit', fn() => transfer_result(load_url($base . '/large', [CURLOPT_MAXFILESIZE => 100])));
run('basic', fn() => transfer_result(load_url($base . '/echo', [
    CURLOPT_USERPWD => 'user:password', CURLOPT_HTTPAUTH => CURLAUTH_BASIC,
])));
run('tls', fn() => transfer_result(load_url($tls . '/echo', [
    CURLOPT_RESOLVE => ['localhost:' . parse_url($tls, PHP_URL_PORT) . ':127.0.0.1'],
    CURLOPT_CAINFO => $ca, CURLOPT_HTTP_VERSION => CURL_HTTP_VERSION_2_0,
])));
run('cookies', function () use ($base) {
    $handle = load_url($base . '/cookie', [CURLOPT_COOKIEFILE => '']);
    $first = curl_exec($handle);
    $cookies = curl_getinfo($handle, CURLINFO_COOKIELIST);
    curl_setopt($handle, CURLOPT_URL, $base . '/echo');
    return [$first, $cookies, curl_exec($handle)];
});
run('share-cookies', function () use ($base) {
    $share = curl_share_init();
    curl_share_setopt($share, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE);
    $first = load_url($base . '/cookie', [CURLOPT_SHARE => $share]);
    $second = load_url($base . '/echo', [CURLOPT_SHARE => $share]);
    $disabled = [curl_exec($first), curl_exec($second)];
    curl_setopt($first, CURLOPT_COOKIEFILE, '');
    curl_setopt($second, CURLOPT_COOKIEFILE, '');
    return [$disabled, curl_exec($first), curl_exec($second)];
});
run('multipart', function () use ($base, $tmp) {
    $path = $tmp . '/file.txt';
    file_put_contents($path, 'disk content');
    return transfer_result(load_url($base . '/multipart', [
        CURLOPT_POSTFIELDS => ['a' => 'text', 'b' => curl_file_create($path, 'text/plain', 'b.txt'),
            'c' => new CURLStringFile('memory content', 'c.txt', 'text/plain')],
    ]));
});
run('file', function () use ($base, $tmp) {
    $path = $tmp . '/output.txt';
    $file = fopen($path, 'wb');
    $handle = load_url($base . '/large');
    curl_setopt($handle, CURLOPT_FILE, $file);
    $ok = curl_exec($handle);
    fclose($file);
    return [$ok, filesize($path), hash_file('sha256', $path)];
});
run('callbacks', function () use ($base) {
    $body = '';
    $header = '';
    $handle = load_url($base . '/echo');
    curl_setopt_array($handle, [
        CURLOPT_HEADERFUNCTION => function ($h, $data) use (&$header) {
            $header .= $data;
            return strlen($data);
        },
        CURLOPT_WRITEFUNCTION => function ($h, $data) use (&$body) {
            $body .= $data;
            return strlen($data);
        },
    ]);
    return [curl_exec($handle), $body, strstr($header, "\r\n", true),
        str_contains($header, "content-type: application/octet-stream\r\n")];
});
run('multi', function () use ($base) {
    $multi = curl_multi_init();
    $handles = [];
    foreach (range(1, 10) as $i) {
        $handles[] = $handle = load_url($base . '/echo', [CURLOPT_HTTPHEADER => ['X-Test: ' . $i]]);
        curl_multi_add_handle($multi, $handle);
    }
    $loops = 0;
    do {
        curl_multi_exec($multi, $running);
        if ($running) {
            curl_multi_select($multi, 1.0);
        }
        if (++$loops > 500) { throw new RuntimeException('multi made no progress'); }
    } while ($running);
    $results = [];
    while ($message = curl_multi_info_read($multi, $queued)) {
        $results[count($results)] = $message['result'];
    }
    return [count(curl_multi_get_handles($multi)), $queued, $results,
        array_map(curl_multi_getcontent(...), $handles)];
});

run('cross-origin-credentials', function () use ($base) {
    $port = parse_url($base, PHP_URL_PORT);
    $options = [
        CURLOPT_FOLLOWLOCATION => true,
        CURLOPT_RESOLVE => ["localhost:$port:127.0.0.1"],
        CURLOPT_HTTPHEADER => ['Authorization: Bearer private', 'Cookie: private=1'],
    ];
    $restricted = transfer_result(load_url($base . '/cross', $options));
    $allowed = transfer_result(load_url($base . '/cross', $options + [
        CURLOPT_UNRESTRICTED_AUTH => true,
    ]));
    return [$restricted, $allowed];
});
run('proxy-http', fn() => transfer_result(load_url('http://localhost/echo', [
    CURLOPT_PROXY => $base, CURLOPT_NOPROXY => '',
])));
run('proxy-connect', fn() => transfer_result(load_url($tls . '/echo', [
    CURLOPT_PROXY => $base, CURLOPT_NOPROXY => '', CURLOPT_CAINFO => $ca,
])));
run('unix-socket', fn() => transfer_result(load_url('http://localhost/echo', [
    CURLOPT_UNIX_SOCKET_PATH => $unix,
])));
run('connect-to', fn() => transfer_result(load_url('http://localhost/echo', [
    CURLOPT_CONNECT_TO => ['localhost:80:127.0.0.1:' . parse_url($base, PHP_URL_PORT)],
])));
run('bearer', fn() => transfer_result(load_url($base . '/echo', [
    CURLOPT_USERPWD => 'user:password', CURLOPT_HTTPAUTH => CURLAUTH_BEARER,
    CURLOPT_XOAUTH2_BEARER => 'opaque-token',
])));
run('read-callback', fn() => transfer_result(load_url($base . '/echo', [
    CURLOPT_UPLOAD => true, CURLOPT_INFILESIZE => 6,
    CURLOPT_READFUNCTION => fn($handle, $stream, $bytes) => substr("abc\0de", 0, $bytes),
])));
run('keep-alive', function () use ($base) {
    $handle = load_url($base . '/echo');
    $one = curl_exec($handle);
    $port = curl_getinfo($handle, CURLINFO_LOCAL_PORT);
    $two = curl_exec($handle);
    return [$one, $two, $port > 0 && $port === curl_getinfo($handle, CURLINFO_LOCAL_PORT)];
});

run('progress-abort', function () use ($base) {
    $calls = [];
    $handle = load_url($base . '/echo', [
        CURLOPT_NOPROGRESS => false,
        CURLOPT_PROGRESSFUNCTION => function ($h, ...$counts) use (&$calls) {
            $calls[] = $counts;
            return 1;
        },
    ]);
    return [curl_exec($handle), curl_errno($handle), curl_error($handle), $calls];
});
run('suspending-write-callback', function () use ($base) {
    $handle = load_url($base . '/echo', [
        CURLOPT_WRITEFUNCTION => static function ($h, $data) {
            Fiber::suspend($data);
            return strlen($data);
        },
    ]);
    $fiber = new Fiber(static fn() => curl_exec($handle));
    return [$fiber->start(), $fiber->resume(), $fiber->getReturn()];
});
