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
file_put_contents("$dir/version.txt", PHP_VERSION."; Unicode 17.0\n");
echo "Generated Unicode 17 and encoding tables from PHP ", PHP_VERSION, "\n";
$entities = '';
foreach (get_html_translation_table(HTML_ENTITIES, ENT_QUOTES|ENT_HTML401, 'UTF-8') as $c => $n) {
    $entities .= mb_ord($c).' '.$n."\n";
}
file_put_contents("$dir/entities.txt", $entities);
