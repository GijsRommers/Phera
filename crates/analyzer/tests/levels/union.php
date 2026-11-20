<?php
final class Left
{
    public string $name = '';
    public function label(): string { return $this->name; }
}
final class Right {}
function takeText(string $value): void {}
function takeAnything(mixed $value): void {}
function receivers(Left|Right $object, int|Left $partial, int|string $scalar): void
{
    takeAnything($object->label());
    takeAnything($object->name);
    takeAnything($partial->label());
    takeAnything($partial->name);
    takeText($scalar);
}
