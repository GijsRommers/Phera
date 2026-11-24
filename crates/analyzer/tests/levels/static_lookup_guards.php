<?php
declare(strict_types=1);

class OptionalLookup
{
    public static function find(string $key): ?self
    {
        return $key === '' ? null : new self();
    }

    public function id(): int { return 42; }
}

function lookupID(string $key, string $otherKey): ?int
{
    if (OptionalLookup::find($key) === null) {
        return null;
    }

    $id = OptionalLookup::find($key)->id();
    $key = $otherKey;
    OptionalLookup::find($key)->id();
    return $id;
}
