--TEST--
HTML and mbstring functions honor the default charset
--FILE--
<?php
ini_set('default_charset', 'ISO-8859-1');
var_dump(bin2hex(htmlspecialchars("\xe9&")), bin2hex(html_entity_decode('&eacute;')));
var_dump(mb_internal_encoding(), mb_strlen('é'));
ini_restore('default_charset');
var_dump(mb_internal_encoding(), mb_strlen('é'));
--EXPECT--
string(12) "e926616d703b"
string(2) "e9"
string(10) "ISO-8859-1"
int(2)
string(5) "UTF-8"
int(1)
