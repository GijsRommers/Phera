<?php
// Reference mappings include PHP's JIS variants and malformed-byte consumption.
if (PHP_VERSION_ID < 80500 || PHP_VERSION_ID >= 80600) throw new Exception('PHP 8.5 required');
$dir = $argv[1];
mb_substitute_character(0xFFFF);
foreach (['EUC-JP','SJIS','CP932','SJIS-win','CP936','BIG-5','EUC-KR'] as $name) {
    $table = '';
    for ($pair = 0; $pair < 65536; $pair++) {
        $out = mb_convert_encoding(pack('n', $pair), 'UTF-32LE', $name);
        $point = unpack('V', $out)[1];
        if ($point == 0xFFFF) $point = 0x7FFFFFFF;
        $table .= pack('V', $point | (strlen($out) == 4 ? 0x80000000 : 0));
    }
    file_put_contents("$dir/decode-$name.bin", $table);
    $table = '';
    for ($cp = 0; $cp <= 0x10FFFF; $cp++) {
        $bytes = mb_chr($cp, $name);
        if ($bytes !== false) $table .= pack('V', $cp).chr(strlen($bytes)).str_pad($bytes, 4, "\0");
    }
    file_put_contents("$dir/encode-$name.bin", $table);
}
