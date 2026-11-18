<?php
$f = fopen('php://memory', 'w+');
fwrite($f, "42 0xff -12.5 tag\n1.25,66\nfoobar 55\n9 wrong\n");
rewind($f);
var_dump(fscanf($f, '%d %x %f %s'));
$a = $b = null;
var_dump(fscanf($f, '%f,%d', $a, $b), $a, $b);
var_dump(fscanf($f, '%2$s %1$d'));
var_dump(fscanf($f, '%d %d'));
var_dump(fscanf($f, '%s'));
