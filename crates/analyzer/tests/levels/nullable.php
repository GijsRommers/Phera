<?php
final class Thing
{
    public string $name = '';
    public function label(): string { return $this->name; }
}
function takeThing(Thing $value): void {}
function takeAnything(mixed $value): void {}
function nullable(?Thing $maybe): void
{
    takeAnything($maybe->label());
    takeAnything($maybe->name);
    takeThing($maybe);
    takeAnything($maybe?->label());
    if ($maybe) {
        takeThing($maybe);
    }
}
