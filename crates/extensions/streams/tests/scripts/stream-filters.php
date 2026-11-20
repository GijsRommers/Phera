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
    echo "zlib.deflate/$chunk: ", bin2hex(gzinflate(stream_get_contents($h))), "\n";
    fclose($h);
}
