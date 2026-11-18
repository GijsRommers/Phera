<?php
foreach ([
    'data:,a+b%00%FF',
    'data://text/plain;charset=UTF-8;base64,AAECAw==',
    'data:text/plain;foo=bar,abc',
    'data:bad,abc',
    'data:text/plain;bad,abc',
] as $uri) {
    $file = fopen($uri, 'r');
    if ($file !== false) {
        var_dump(stream_get_meta_data($file), stream_is_local($file));
        var_dump(bin2hex(stream_get_contents($file)), fclose($file));
    }
}
foreach (['php://memory', 'php://temp', 'php://input', 'php://output'] as $uri) {
    $file = fopen($uri, 'w+');
    var_dump(stream_get_meta_data($file));
    var_dump(stream_set_timeout($file, 1));
    var_dump(stream_set_read_buffer($file, 0), stream_set_write_buffer($file, 0));
    var_dump(stream_isatty($file));
}
var_dump(file_get_contents('file://' . __FILE__, length: 5));
var_dump(is_dir(sys_get_temp_dir()));
