<?php
$many = str_repeat("x", 5 * 1024 * 1024);
$f = fopen('php://temp/maxmemory:1024', 'w+');
var_dump(fwrite($f, $many), ftell($f));
rewind($f);
var_dump(strlen(fgets($f)), feof($f));
rewind($f);
var_dump(file_put_contents('data', $f));
var_dump(strlen(file('data')[0]));
var_dump(readfile('data'));
rewind($f);
var_dump(fpassthru($f));
var_dump(fclose($f));
