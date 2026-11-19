--TEST--
ReflectionObject probes existence without invoking magic and retries failed lazy initialization
--FILE--
<?php
class Entity {
    public int $id;
    public function __isset($name) { echo "magic isset\n"; return true; }
}
$r = new ReflectionClass(Entity::class);
$attempts = 0;
$o = $r->newLazyGhost(function ($o) use (&$attempts) {
    echo "initialize\n";
    if (++$attempts === 1) {
        throw new Exception('temporary error');
    }
    $o->id = 1;
});
$ro = new ReflectionObject($o);
var_dump($ro->hasProperty('missing'), $r->isUninitializedLazyObject($o));
try { $ro->getProperties(); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
var_dump($r->isUninitializedLazyObject($o));
var_dump(count($ro->getProperties()), $r->isUninitializedLazyObject($o));

--EXPECT--
bool(false)
bool(true)
initialize
temporary error
bool(true)
initialize
int(1)
bool(false)
