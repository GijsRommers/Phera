--TEST--
Variance must load classes declared in unexecuted loader branches
--FILE--
<?php
class ParentType {
    public function value(): ResultBase { return new ResultBase; }
}
spl_autoload_register(function (string $name) {
    if ($name === 'ResultBase') {
        echo "load ResultBase\n";
        class ResultBase {}
    }
    if ($name === 'ResultChild') {
        class ResultChild extends ResultBase {}
        echo "load ResultChild\n";
    }
});
class ChildType extends ParentType {
    public function value(): ResultChild { return new ResultChild; }
}
echo get_class((new ChildType)->value()), "\n";
--EXPECT--
load ResultBase
load ResultChild
ResultChild
