$handle = curl_init();
$prefixes = ['', "\0", "\xff", '%', '%2', '+', 'abc-', "😺"];
foreach (range(0, 255) as $i) {
    foreach ($prefixes as $key => $prefix) {
        $bytes = $prefix . chr($i);
        run("escape/$key/$i", fn() => curl_escape($handle, $bytes));
        run("unescape/$key/$i", fn() => curl_unescape($handle, $bytes));
    }
}
foreach (range(-1, 105) as $i) {
    // Some libcurl packages predate the codes beyond 99.
    if ($i >= 100 && $i <= 101 && !in_array((string)$i, $knownNewErrors, true)) {
        run("strerror/$i/older-backend", fn() => is_string(curl_strerror($i)));
    } else {
        run("strerror/$i", fn() => curl_strerror($i));
    }
}
foreach (range(-1, 14) as $i) {
    run("multi_strerror/$i", fn() => curl_multi_strerror($i));
    run("share_strerror/$i", fn() => curl_share_strerror($i));
}
run('init', fn() => curl_init());
run('init/nul', fn() => curl_init("\0"));
run('getinfo/init', function () use ($oracleFields) {
    $info = curl_getinfo(curl_init());
    // Default paths depend on the oracle's TLS backend and its system package.
    foreach (['cainfo', 'capath'] as $path) {
        if (isset($info[$path]) && is_string($info[$path])) { $info[$path] = ''; }
    }
    return array_intersect_key($info, array_fill_keys($oracleFields, true));
});
run('getinfo/option-invalid', fn() => curl_getinfo($handle, 9000));
run('getinfo/private', function () use ($handle) {
    curl_setopt($handle, CURLOPT_PRIVATE, ['keep' => 2]);
    return curl_getinfo($handle, CURLINFO_PRIVATE);
});
run('setopt/invalid', fn() => curl_setopt($handle, 999999, true));
run('setopt/nul', fn() => curl_setopt($handle, CURLOPT_URL, "https://local\0host/"));
run('setopt_array/invalid', fn() => curl_setopt_array($handle, ['bad' => false]));
run('copy', function () {
    $handle = curl_init('http://127.0.0.1');
    curl_setopt($handle, CURLOPT_CUSTOMREQUEST, 'PATCH');
    $other = curl_copy_handle($handle);
    curl_reset($handle);
    return [
        get_class($other),
        curl_getinfo($other, CURLINFO_EFFECTIVE_URL),
        curl_getinfo($other, CURLINFO_EFFECTIVE_METHOD),
        curl_getinfo($handle, CURLINFO_EFFECTIVE_URL)
    ];
});
run('empty-transfer', function () {
    $handle = curl_init();
    return [curl_exec($handle), curl_errno($handle), curl_error($handle)];
});
run('upkeep', fn() => curl_upkeep($handle));
run('pause', fn() => curl_pause($handle, CURLPAUSE_CONT));
run('close', fn() => curl_close($handle));
run('multi-init', fn() => curl_multi_init());
run('multi-empty', function () {
    $multi = curl_multi_init();
    $queued = 21;
    return [curl_multi_select($multi, 0), curl_multi_exec($multi, $running), $running,
        curl_multi_errno($multi), curl_multi_info_read($multi, $queued), $queued,
        curl_multi_get_handles($multi), curl_multi_close($multi)];
});
run('multi-invalid-timeout', fn() => curl_multi_select(curl_multi_init(), NAN));
run('share-init', fn() => curl_share_init());
run('share-cookie', function () {
    $share = curl_share_init();
    return [curl_share_setopt($share, CURLSHOPT_SHARE, CURL_LOCK_DATA_COOKIE),
        curl_share_errno($share), curl_share_close($share)];
});
run('persistent-dns', fn() => curl_share_init_persistent([CURL_LOCK_DATA_DNS]));
run('persistent-cookie', fn() => curl_share_init_persistent([CURL_LOCK_DATA_COOKIE]));
run('file-create', fn() => curl_file_create('test.txt'));
run('file-accessors', function () {
    $file = new CURLFile('test.txt', posted_filename: 'payload.txt');
    $file->setMimeType('text/plain');
    $file->setPostFilename('send.txt');
    return [$file->getFilename(), $file->getMimeType(), $file->getPostFilename(), $file];
});
run('string-file', fn() => new CURLStringFile("raw\0bytes", 'test.txt'));
$classes = ['CurlHandle', 'CurlMultiHandle', 'CurlShareHandle', 'CurlSharePersistentHandle'];
foreach ($classes as $class) {
    run("construct/$class", fn() => new $class());
}
run('integer-cast', function () {
    $a = curl_init();
    $b = curl_multi_init();
    return [(int)$a === spl_object_id($a), (int)$b === spl_object_id($b)];
});
run('safe-upload', fn() => curl_setopt($handle, CURLOPT_SAFE_UPLOAD, false));
run('invalid-callback', fn() => curl_setopt($handle, CURLOPT_HEADERFUNCTION, 'undefined'));
run('invalid-array-callback', fn() => curl_setopt_array($handle, [
    CURLOPT_WRITEFUNCTION => 'undefined',
]));
