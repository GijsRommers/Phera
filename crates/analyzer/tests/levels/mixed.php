<?php
function takeText(string $value): void {}
function takeAnything(mixed $value): void {}
function explicit(mixed $value): void
{
    takeAnything($value);
    takeText($value);
    takeAnything($value->name);
    takeAnything($value->label());
    takeAnything($value[0]);
    takeAnything($value + 1);
    $value();
    $value::label();
    print $value;
    echo $value;
    foreach ($value as $item) { takeAnything($item); }
    throw $value;
}
/** @phpstan-param mixed $value */
function documented($value): void
{
    takeText($value);
    takeAnything($value->name);
}
function implicit($value): void
{
    takeAnything($value);
    takeText($value);
    takeAnything($value->name);
    takeAnything($value->label());
}
