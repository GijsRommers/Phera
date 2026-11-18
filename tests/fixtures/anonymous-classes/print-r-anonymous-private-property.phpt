--TEST--
anonymous-classes: print r anonymous private property
--FILE--
<?php
print_r(new class {private $x=1;});
--EXPECT--
class@anonymous Object
(
    [x:class@anonymous:private] => 1
)
