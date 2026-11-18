--TEST--
anonymous-classes: instanceof interface
--FILE--
<?php
interface I {} var_dump(new class implements I {} instanceof I);
--EXPECT--
bool(true)
