--TEST--
Object frames include the receiver before the call operator
--FILE--
<?php
class ParentClass {
    function inspect() {
        $frame = debug_backtrace()[0];
        echo implode(',', array_keys($frame)), "\n";
        var_dump($frame['object'] === $this, $frame['class'], $frame['type']);
        var_dump(array_key_exists('object', debug_backtrace(0)[0]));
    }
}
class ChildClass extends ParentClass {}
(new ChildClass)->inspect();
--EXPECT--
file,line,function,class,object,type,args
bool(true)
string(11) "ParentClass"
string(2) "->"
bool(false)
