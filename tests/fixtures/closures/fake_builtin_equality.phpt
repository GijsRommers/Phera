--TEST--
fake builtin equality
--FILE--
<?php
$a=strlen(...);$b=STRLEN(...);var_dump($a==$b,$a===$b,$a==clone $a,$a!=strrev(...));
--EXPECT--
bool(true)
bool(false)
bool(true)
bool(true)
