--TEST--
Native classes: constant-match
--FILE--
<?php
var_dump(match (2) {
 PDO::FETCH_ASSOC=>"assoc", PDO::FETCH_NUM=>"num", default=>"other" }
);

--EXPECT--
string(5) "assoc"
