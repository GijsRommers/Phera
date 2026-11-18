--TEST--
An anonymous class dispatches enum offsets to trait methods
--FILE--
<?php
enum Field { case Title; }
trait FieldAccess {
    private array $data = [];
    public function offsetExists(mixed $offset): bool { return isset($this->data[$offset->name]); }
    public function offsetGet(mixed $offset): mixed { return $this->data[$offset->name]; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->data[$offset->name] = $value; }
    public function offsetUnset(mixed $offset): void { unset($this->data[$offset->name]); }
}
$record = new class implements ArrayAccess { use FieldAccess; };
$record[Field::Title] = 'Entry';
var_dump(isset($record[Field::Title]), $record[Field::Title]);
unset($record[Field::Title]);
var_dump(isset($record[Field::Title]));
--EXPECT--
bool(true)
string(5) "Entry"
bool(false)
