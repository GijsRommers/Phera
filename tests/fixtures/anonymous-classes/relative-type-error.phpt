--TEST--
anonymous-classes: relative type error
--FILE--
<?php
$object = new class {
    function test(): self|string { return new stdClass; }
};
try { $object->test(); } catch (TypeError $error) { echo $error->getMessage(); }
--EXPECT--
class@anonymous::test(): Return value must be of type class@anonymous|string, stdClass returned
