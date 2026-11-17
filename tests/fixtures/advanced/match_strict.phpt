--TEST--
match_strict
--FILE--
<?php
echo match('1'){1=>'int','1'=>'string'};
--EXPECT--
string