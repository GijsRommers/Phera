--TEST--
Application class sweep: collection
--FILE--
<?php
class Collection {
private array $items=[];
function add(
object $item)
: static {
$this->items[]=$item;
return $this;
}
function all(
)
: array {
return $this->items;
}
function size(
)
: int {
return count($this->items);
}
}
 class Item {
function __construct(public string $name){
}
}
 $items=(new Collection)->add(new Item('one'))->add(new Item('two'));
echo $items->size(),
"\n";
foreach($items->all() as $item){
echo $item->name,
"\n";
}

--EXPECT--
2
one
two
