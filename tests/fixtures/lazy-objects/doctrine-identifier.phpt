--TEST--
A Doctrine-shaped ghost exposes its identifier before loading other properties
--FILE--
<?php
class Entity {
    private int $id;
    public string $name;
    public function id(): int { return $this->id; }
}
$class = new ReflectionClass(Entity::class);
$entity = $class->newLazyGhost(static function (Entity $ghost) {
    echo "load ", $ghost->id(), "
";
    $ghost->name = 'Gijs';
});
$id = new ReflectionProperty(Entity::class, 'id');
$id->setRawValueWithoutLazyInitialization($entity, 17);
var_dump($id->isLazy($entity), $class->isUninitializedLazyObject($entity));
echo $entity->id(), "
";
echo $entity->name, "
";
--EXPECT--
bool(false)
bool(true)
17
load 17
Gijs
