--TEST--
anonymous-classes: instanceof parent
--FILE--
<?php
class P {} var_dump(new class extends P {} instanceof P);
--EXPECT--
bool(true)
