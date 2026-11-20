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
