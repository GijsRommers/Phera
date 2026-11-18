<?php
$alphabet = '';
for ($i = 0; $i < 256; $i++) {
    $alphabet .= chr($i);
}
$binary = str_repeat($alphabet, 20480);
var_dump(file_put_contents('a', $binary));
var_dump(md5(file_get_contents('a')));
$f = fopen('a', 'r');
var_dump(md5(fread($f, strlen($binary))), ftell($f), feof($f));
var_dump(fread($f, 1), feof($f));
rewind($f);
$g = fopen('b', 'w+');
var_dump(fwrite($g, $binary));
rewind($g);
var_dump(md5(stream_get_contents($g)));
rewind($g);
var_dump(stream_copy_to_stream($f, $g));
var_dump(fflush($g), fsync($g), fdatasync($g));
var_dump(copy('a', 'copy'), rename('copy', 'renamed'));
var_dump(file_put_contents('b', ['1', 2, "\0", false], FILE_APPEND | LOCK_EX));
var_dump(bin2hex(file_get_contents('b', offset: -4)));
