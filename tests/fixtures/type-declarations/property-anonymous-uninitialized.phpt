--TEST--
Uninitialized anonymous property errors use the displayed class name
--FILE--
<?php
$object = new class {
    public int $value;
};
try {
    var_dump($object->value);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Typed property class@anonymous::$value must not be accessed before initialization
