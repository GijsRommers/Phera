--TEST--
Changing the PCRE limit also changes a previously cached pattern
--FILE--
<?php
$pattern = '/(*NO_START_OPT)a(?:a+)+b/';
preg_match($pattern, 'aaaaaaaaaaaaaaaac');
ini_set('pcre.jit', '0');
ini_set('pcre.backtrack_limit', '1');
var_dump(preg_match($pattern, 'aaaaaaaaaaaaaaaac'), preg_last_error());
--EXPECT--
bool(false)
int(2)
