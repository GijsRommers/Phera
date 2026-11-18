--TEST--
Self-referential array passed to extract
--FILE--
<?php
$a = [];
$a['self'] = &$a;
var_dump(extract($a, EXTR_REFS));
$self['new'] = 1;
var_dump($a['new']);
--EXPECT--
int(1)
int(1)
