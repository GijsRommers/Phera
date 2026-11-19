--TEST--
A throwing ghost rolls back its writes and retries
--FILE--
<?php
class Item {
    public int $id;
    public int $count = 3;
}
$class = new ReflectionClass(Item::class);
$attempts = 0;
$item = $class->newLazyGhost(function (Item $self) use (&$attempts) {
    echo $self->count, "
";
    $self->id = 8;
    $self->count = 4;
    if (++$attempts === 1) {
        throw new Exception('again');
    }
});
try {
    echo $item->id;
} catch (Exception $exception) {
    echo $exception->getMessage(), "
";
}
var_dump($class->isUninitializedLazyObject($item));
echo $item->id, "
";
--EXPECT--
3
again
bool(true)
3
8
