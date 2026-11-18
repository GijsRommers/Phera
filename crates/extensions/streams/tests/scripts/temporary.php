<?php
$f = tmpfile();
$uri = stream_get_meta_data($f)['uri'];
var_dump(is_file($uri), fwrite($f, "temp\0data"), ftell($f));
rewind($f);
var_dump(bin2hex(stream_get_contents($f)));
$alias = $f;
var_dump(fclose($alias), get_resource_type($f), get_debug_type($f), is_file($uri));
$name = tempnam('.', 'name-');
var_dump(is_file($name), fileperms($name) & 0777);
var_dump(unlink($name));
