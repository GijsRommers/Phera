<?php
// PHP 8.5 / Unicode 17 reference tables; no runtime PHP dependency.
if (PHP_VERSION_ID < 80500 || PHP_VERSION_ID >= 80600) throw new Exception('PHP 8.5 required');
$dir = $argv[1];
$maps = array_fill(0, 8, '');
$width = ''; $properties = '';
for ($cp = 0; $cp <= 0x10FFFF; $cp++) {
    if ($cp >= 0xD800 && $cp <= 0xDFFF) continue;
    $s = mb_chr($cp, 'UTF-8');
    foreach ($maps as $mode => $_) {
        $out = mb_convert_case($s, $mode, 'UTF-8');
        if ($out !== $s) {
            $points = array_map(fn($c) => mb_ord($c), mb_str_split($out));
            $maps[$mode] .= pack('V4', $cp, ...array_pad($points, 3, 0xFFFFFFFF));
        }
    }
    if (mb_strwidth($s) == 2) $width .= pack('V', $cp);
    // Infer title-casing boundary properties through the actual PHP algorithm.
    $first = mb_convert_case('1'.$s.'B', MB_CASE_TITLE);
    $second = mb_convert_case('A'.$s.'B', MB_CASE_TITLE);
    $flags = (str_ends_with($first, 'b') ? 1 : 0) |
        (str_ends_with($second, 'b') && str_ends_with($first, 'B') ? 2 : 0);
    if ($flags) $properties .= pack('VC', $cp, $flags);
}
foreach ($maps as $mode => $map) file_put_contents("$dir/case-$mode.bin", $map);
file_put_contents("$dir/width.bin", $width);
file_put_contents("$dir/properties.bin", $properties);
$names = ['7bit','8bit','UCS-4','UCS-4BE','UCS-4LE','UCS-2','UCS-2BE','UCS-2LE',
    'UTF-32','UTF-32BE','UTF-32LE','UTF-16','UTF-16BE','UTF-16LE','UTF-8','ASCII',
    'EUC-JP','SJIS','CP932','SJIS-win','ISO-2022-JP','GB18030','Windows-1252',
    'Windows-1254','ISO-8859-1','ISO-8859-2','ISO-8859-3','ISO-8859-4','ISO-8859-5',
    'ISO-8859-6','ISO-8859-7','ISO-8859-8','ISO-8859-9','ISO-8859-10','ISO-8859-13',
    'ISO-8859-14','ISO-8859-15','ISO-8859-16','CP936','BIG-5','EUC-KR',
    'Windows-1251','KOI8-R','KOI8-U'];
$metadata = '';
foreach ($names as $i => $name) {
    $metadata .= $name.'|'.implode(',', mb_encoding_aliases($name)).'|'.
        (mb_preferred_mime_name($name) ?: '')."\n";
    if (str_starts_with($name,'ISO-8859-') || str_starts_with($name,'Windows-') ||
        str_starts_with($name,'KOI8-')) {
        $table = '';
        for ($byte = 0; $byte < 256; $byte++) {
            $s = chr($byte);
            $point = mb_check_encoding($s, $name) ? mb_ord($s, $name) : 0xFFFFFFFF;
            $table .= pack('V', $point);
        }
        file_put_contents("$dir/single-$i.bin", $table);
    }
}
file_put_contents("$dir/encodings.txt", $metadata);
file_put_contents("$dir/version.txt", PHP_VERSION."; Unicode 17.0\n");
echo "Generated Unicode 17 and encoding tables from PHP ", PHP_VERSION, "\n";
// Deprecated transfer metadata is kept after the character encodings for stable table IDs.
file_put_contents("$dir/encodings.txt", "BASE64||BASE64\nUUENCODE||x-uuencode\n".
    "HTML-ENTITIES|HTML,html|HTML-ENTITIES\n" .
    "Quoted-Printable|qprint|Quoted-Printable\n", FILE_APPEND);
$entities = '';
foreach (get_html_translation_table(HTML_ENTITIES, ENT_QUOTES|ENT_HTML401, 'UTF-8') as $c => $n) {
    $entities .= mb_ord($c).' '.$n."\n";
}
file_put_contents("$dir/entities.txt", $entities);
