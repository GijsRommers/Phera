--TEST--
enums: ordinary constant
--FILE--
<?php
enum E{const TEXT="ok";case A;} echo E::TEXT;
--EXPECT--
ok
