--TEST--
Null returns preserve destructor order, finally, and distinct return protocols
--FILE--
<?php
class Held {
    public function __construct(public string $label) {}
    public function __destruct() { echo "drop ", $this->label, "\n"; }
}

function fallThrough() {
    $local = new Held('local');
    echo "function body\n";
}

function fallThroughFinally() {
    $local = new Held('finally');
    try {
        echo "try body\n";
    } finally {
        echo "finally body\n";
    }
}

function returnThroughFinally() {
    $local = new Held('explicit');
    try {
        return null;
    } finally {
        echo "explicit finally\n";
    }
}

function generate(): Generator {
    $local = new Held('generator');
    yield 1;
    echo "generator body\n";
}

class Constructed {
    public function __construct() {
        $local = new Held('constructor');
        echo "constructor body\n";
    }
}

function &byReference() {
    static $value;
    return $value;
}

function &nullReference() {
    return null;
}

var_dump(fallThrough());
var_dump(fallThroughFinally());
var_dump(returnThroughFinally());
$generator = generate();
var_dump($generator->current());
$generator->next();
var_dump($generator->getReturn());
var_dump((new Constructed()) instanceof Constructed);
$alias =& byReference();
$alias = 'changed';
var_dump(byReference());
set_error_handler(function ($level, $message) { echo $message, "\n"; return true; });
var_dump(nullReference());

function checkedReturn(): int {
    try {
        return null;
    } catch (TypeError $error) {
        echo "caught invalid return\n";
        return 7;
    }
}
var_dump(checkedReturn());
--EXPECT--
function body
drop local
NULL
try body
finally body
drop finally
NULL
explicit finally
drop explicit
NULL
int(1)
generator body
drop generator
NULL
constructor body
drop constructor
bool(true)
string(7) "changed"
Only variable references should be returned by reference
NULL
caught invalid return
int(7)
