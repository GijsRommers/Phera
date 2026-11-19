<?php
// Reference tables from both PHP oracles. PHP is only needed to regenerate them.
$unicode = match (PHP_MAJOR_VERSION . '.' . PHP_MINOR_VERSION) {
    '8.5' => '17.0',
    '8.6' => '18.0',
    default => throw new Exception('PHP 8.5 or 8.6 required'),
};
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
file_put_contents("$dir/version.txt", PHP_VERSION . "; Unicode $unicode\n");
echo "Generated Unicode $unicode tables from PHP ", PHP_VERSION, "\n";
if ($unicode === '18.0') return;

// Single-byte tables; Phera's own list of names is in src/encoding.rs.
foreach (mb_list_encodings() as $name) {
    if (str_starts_with($name,'ISO-8859-') || str_starts_with($name,'Windows-') ||
        str_starts_with($name,'KOI8-')) {
        $table = '';
        for ($byte = 0; $byte < 256; $byte++) {
            $s = chr($byte);
            $point = mb_check_encoding($s, $name) ? mb_ord($s, $name) : 0xFFFFFFFF;
            $table .= pack('V', $point);
        }
        file_put_contents("$dir/single-$name.bin", $table);
    }
}
$entities = '';
foreach (get_html_translation_table(HTML_ENTITIES, ENT_QUOTES|ENT_HTML401, 'UTF-8') as $c => $n) {
    $entities .= mb_ord($c).' '.$n."\n";
}
file_put_contents("$dir/entities.txt", $entities);
