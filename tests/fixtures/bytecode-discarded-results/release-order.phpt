--TEST--
Discarded call results release in PHP statement order
--FILE--
<?php
class ResultOwner {
    function __construct(public string $label) {
        echo "new $this->label\n";
    }

    function __destruct() {
        echo "drop $this->label\n";
    }

    function selfResult(): self {
        echo "return this\n";
        return $this;
    }
}

function temporaryResult($argument = null): ResultOwner {
    $local = new ResultOwner('callee local');
    return new ResultOwner('result');
}

function variableResult(): ResultOwner {
    $first = new ResultOwner('first local');
    $last = new ResultOwner('returned local');
    return $last;
}

function &referenceResult(): ResultOwner {
    $held = new ResultOwner('referenced local');
    return $held;
}

function generatorResult(): Generator {
    echo "generator body\n";
    yield new ResultOwner('yielded value');
}

unserialize('O:11:"ResultOwner":1:{s:5:"label";s:13:"native result";}');
echo "after native result\n";
temporaryResult(new ResultOwner('argument'));
echo "after temporary result\n";
variableResult();
echo "after variable result\n";
referenceResult();
echo "after reference result\n";
generatorResult();
echo "after generator result\n";
new ResultOwner('unused new');
echo "after new\n";
(new ResultOwner('temporary receiver'))->selfResult();
echo "after receiver\n";
$object = new ResultOwner('named receiver');
$object->selfResult();
echo "after named receiver\n";
unset($object);

try {
    temporaryResult();
    throw new RuntimeException('caught');
} catch (RuntimeException $exception) {
    echo $exception->getMessage(), "\n";
} finally {
    echo "finally\n";
}

class ThrowOnDrop {
    function __destruct() {
        echo "throw from destructor\n";
        throw new LogicException('destructor error');
    }
}
function throwingResult(): ThrowOnDrop {
    $held = new ResultOwner('throwing callee local');
    return new ThrowOnDrop;
}
try {
    throwingResult();
    echo "unexpected next statement\n";
} catch (LogicException $exception) {
    echo $exception->getMessage(), "\n";
}
--EXPECT--
drop native result
after native result
new argument
new callee local
new result
drop result
drop argument
drop callee local
after temporary result
new first local
new returned local
drop first local
drop returned local
after variable result
new referenced local
drop referenced local
after reference result
after generator result
new unused new
drop unused new
after new
new temporary receiver
return this
drop temporary receiver
after receiver
new named receiver
return this
after named receiver
drop named receiver
new callee local
new result
drop result
drop callee local
caught
finally
new throwing callee local
throw from destructor
drop throwing callee local
destructor error
