--TEST--
instanceof of a name no class of the request has: false without autoloading, then the class once declared or aliased
--FILE--
<?php
namespace App;

spl_autoload_register(function ($name) { echo "autoload $name\n"; });

interface Shown {}
class Entity {}

function check(object $o): string {
    $a = $o instanceof Proxy ? 'proxy' : 'no-proxy';
    $b = $o instanceof \Missing\Thing ? 'thing' : 'no-thing';
    $c = $o instanceof Shown ? 'shown' : 'not-shown';
    return "$a $b $c";
}

$e = new Entity;
for ($i = 0; $i < 3; $i++) {
    echo check($e), "\n";
}
// Declared later in the request: the same sites now see it.
if (true) {
    class Proxy extends Entity implements Shown {}
}
echo check(new Proxy), "\n";
echo check($e), "\n";
class_alias(Entity::class, 'Missing\Thing');
echo check($e), "\n";
var_dump(42 instanceof Proxy, null instanceof Undeclared);

--EXPECTF--
no-proxy no-thing not-shown
no-proxy no-thing not-shown
no-proxy no-thing not-shown
proxy no-thing shown
no-proxy no-thing not-shown
no-proxy thing not-shown
bool(false)
bool(false)
