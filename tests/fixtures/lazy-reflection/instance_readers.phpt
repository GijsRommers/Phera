--TEST--
Reflection readers initialize ghosts and proxies only when reading instance properties
--FILE--
<?php
#[AllowDynamicProperties]
class Entity {
    public static string $static = 'static';
    public int $id;
    public string $name = 'default';
}
$reflector = new ReflectionClass(Entity::class);
$initialize = function ($o) { echo "[init] "; $o->id = 7; $o->dynamic = 'yes'; };
$ghost = fn() => $reflector->newLazyGhost($initialize);
$proxy = fn() => $reflector->newLazyProxy(function () use ($initialize) { $o = new Entity; $initialize($o); return $o; });
$properties = fn($r) => array_map(fn($p) => $p->getName(), $r->getProperties());
foreach (['ghost' => $ghost, 'proxy' => $proxy] as $kind => $make) {
    foreach ([
        'class getProperties' => fn($o) => $properties(new ReflectionClass($o)),
        'object constructor' => fn($o) => (new ReflectionObject($o))->getName(),
        'object static-only' => fn($o) => array_map(fn($p) => $p->getName(), (new ReflectionObject($o))->getProperties(ReflectionProperty::IS_STATIC)),
        'object getProperties' => fn($o) => $properties(new ReflectionObject($o)),
        'class getValue' => fn($o) => $reflector->getProperty('id')->getValue($o),
        'object getProperty' => fn($o) => (new ReflectionObject($o))->getProperty('id')->getName(),
        'object getValue' => fn($o) => (new ReflectionObject($o))->getProperty('id')->getValue($o),
        'object hasProperty' => fn($o) => (new ReflectionObject($o))->hasProperty('id'),
        'object hasProperty dynamic' => fn($o) => (new ReflectionObject($o))->hasProperty('dynamic'),
        'object getProperty dynamic' => fn($o) => (new ReflectionObject($o))->getProperty('dynamic')->getValue($o),
        'object getDefaultProperties' => fn($o) => (new ReflectionObject($o))->getDefaultProperties(),
        'object getStaticProperties' => fn($o) => (new ReflectionObject($o))->getStaticProperties(),
    ] as $name => $read) {
        $o = $make();
        echo $kind, ' ', $name, ' ';
        try { echo json_encode($read($o)); } catch (ReflectionException $e) { echo $e->getMessage(); }
        echo ' lazy: ', json_encode($reflector->isUninitializedLazyObject($o)), "\n";
    }
}

--EXPECT--
ghost class getProperties ["static","id","name"] lazy: true
ghost object constructor "Entity" lazy: true
ghost object static-only ["static"] lazy: true
ghost object getProperties [init] ["static","id","name","dynamic"] lazy: false
ghost class getValue [init] 7 lazy: false
ghost object getProperty "id" lazy: true
ghost object getValue [init] 7 lazy: false
ghost object hasProperty true lazy: true
ghost object hasProperty dynamic [init] true lazy: false
ghost object getProperty dynamic [init] "yes" lazy: false
ghost object getDefaultProperties {"static":"static","name":"default"} lazy: true
ghost object getStaticProperties {"static":"static"} lazy: true
proxy class getProperties ["static","id","name"] lazy: true
proxy object constructor "Entity" lazy: true
proxy object static-only ["static"] lazy: true
proxy object getProperties [init] ["static","id","name","dynamic"] lazy: false
proxy class getValue [init] 7 lazy: false
proxy object getProperty "id" lazy: true
proxy object getValue [init] 7 lazy: false
proxy object hasProperty true lazy: true
proxy object hasProperty dynamic [init] true lazy: false
proxy object getProperty dynamic [init] "yes" lazy: false
proxy object getDefaultProperties {"static":"static","name":"default"} lazy: true
proxy object getStaticProperties {"static":"static"} lazy: true
