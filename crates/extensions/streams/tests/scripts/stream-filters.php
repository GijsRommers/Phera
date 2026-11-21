<?php
foreach (['', 'a', 'abc', "Abcd\x00\xff?"] as $data) {
    foreach ([1, 2, 8192] as $chunk) {
        foreach (['string.rot13', 'string.toupper', 'string.tolower',
            'convert.base64-encode', 'convert.quoted-printable-encode'] as $name) {
            $h = fopen('php://memory', 'w+');
            stream_set_chunk_size($h, $chunk);
            $filter = stream_filter_append($h, $name, STREAM_FILTER_WRITE);
            foreach (str_split($data, $chunk) as $part) {
                echo fwrite($h, $part);
            }
            var_dump(stream_filter_remove($filter));
            rewind($h);
            echo $name, ':', bin2hex(stream_get_contents($h)), "\n";
            fclose($h);
        }
    }
}

$source = tempnam(sys_get_temp_dir(), 'flt');
file_put_contents($source, 'Hello');
foreach (['read=string.rot13|string.toupper', 'convert.base64-encode',
    'read=convert.iconv.UTF-8.UTF-16LE'] as $chain) {
    echo $chain, ':', bin2hex(file_get_contents("php://filter/$chain/resource=$source")), "\n";
}
unlink($source);

$h = fopen('php://memory', 'w+');
$f = stream_filter_append($h, 'convert.base64-encode', STREAM_FILTER_WRITE,
    ['line-length' => 4, 'line-break-chars' => '|']);
fwrite($h, 'abcdefg');
stream_filter_remove($f);
rewind($h);
echo stream_get_contents($h), "\n";
fclose($h);

$h = fopen('php://memory', 'w+');
var_dump(stream_filter_append($h, 'string.strip_tags', STREAM_FILTER_READ));
fclose($h);

$decoded = "first\nsecond\x00\xff";
$cases = [
    ['convert.base64-decode', base64_encode($decoded)],
    ['convert.quoted-printable-decode', "first=0Asecond=00=FF"],
    ['zlib.inflate', gzdeflate($decoded)],
    ['dechunk', "3;note=test\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"],
    ['consumed', 'unused bytes'],
];
foreach ($cases as [$filter, $encoded]) {
    foreach ([1, 2, 8192] as $chunk) {
        $h = fopen('php://memory', 'w+');
        fwrite($h, $encoded);
        rewind($h);
        stream_set_chunk_size($h, $chunk);
        stream_filter_append($h, $filter, STREAM_FILTER_READ);
        echo "$filter/$chunk: ", bin2hex(stream_get_contents($h)), "\n";
        fclose($h);
    }
}

foreach ([1, 3, 8192] as $chunk) {
    $h = fopen('php://memory', 'w+');
    $filter = stream_filter_append($h, 'zlib.deflate', STREAM_FILTER_WRITE, ['level' => 5]);
    foreach (str_split($decoded, $chunk) as $part) {
        fwrite($h, $part);
    }
    stream_filter_remove($filter);
    rewind($h);
    $compressed = stream_get_contents($h);
    echo "zlib.deflate/$chunk: ", bin2hex($compressed), " ", bin2hex(gzinflate($compressed)), "\n";
    fclose($h);
}

$data = "Hello, World!\nSecond line with é and ü\n";
$path = tempnam(sys_get_temp_dir(), 'deflate');
file_put_contents($path, $data);
echo "deflate read: ", bin2hex(file_get_contents("php://filter/read=zlib.deflate/resource=$path")), "\n";
foreach ([null, 0, ['level' => 6, 'window' => 15], ['level' => 6, 'window' => 31]] as $params) {
    $h = fopen($path, 'wb');
    if ($params === null) {
        stream_filter_append($h, 'zlib.deflate', STREAM_FILTER_WRITE);
    } else {
        stream_filter_append($h, 'zlib.deflate', STREAM_FILTER_WRITE, $params);
    }
    fwrite($h, $data);
    fflush($h);
    fwrite($h, $data);
    fclose($h);
    $encoded = file_get_contents($path);
    // Gzip's OS header byte depends on the platform's zlib build.
    if (($params['window'] ?? null) === 31) {
        $encoded[9] = "\0";
    }
    echo "deflate write: ", bin2hex($encoded), "\n";
}
unlink($path);

function iconvBuckets($filter, $chunks, $flush = false)
{
    $h = fopen('php://memory', 'w+');
    $attachment = stream_filter_append($h, $filter, STREAM_FILTER_WRITE);
    foreach ($chunks as $chunk) {
        fwrite($h, $chunk);
        if ($flush) {
            fflush($h);
        }
    }
    stream_filter_remove($attachment);
    rewind($h);
    echo $filter, ': ', bin2hex(stream_get_contents($h)), "\n";
    fclose($h);
}

iconvBuckets('convert.iconv.ISO-2022-JP.UTF-8', ["A\x1b\x24B", 'F|', 'K\\', "\x1b(BZ"]);
iconvBuckets('convert.iconv.UTF-8.ISO-2022-JP', ['A日', '本', 'Z']);
iconvBuckets('convert.iconv.UTF-8.ISO-2022-JP', ['A日', '本']);
iconvBuckets('convert.iconv.UTF-8.ISO-2022-JP', ['A日', '本', 'Z'], true);
