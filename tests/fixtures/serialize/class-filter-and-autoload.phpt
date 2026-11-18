--TEST--
Unserialize autoloads allowed classes and keeps the original blocked class name
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "autoload $name\n";
    class Available { public $n; }
});
$data = 'O:9:"Available":1:{s:1:"n";i:4;}';
$x = unserialize($data, ['allowed_classes' => false]);
var_dump(get_class($x), serialize($x) === $data);
$x = unserialize($data, ['allowed_classes' => ['aVaIlAbLe']]);
var_dump(get_class($x), $x->n);
?>
--EXPECT--
string(22) "__PHP_Incomplete_Class"
bool(true)
autoload Available
string(9) "Available"
int(4)
