--TEST--
enums: cases empty
--FILE--
<?php
enum E{} var_dump(E::cases());
--EXPECT--
array(0) {
}
