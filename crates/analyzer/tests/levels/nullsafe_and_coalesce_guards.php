<?php

final class GuardedValue
{
    public function live(): bool { return true; }
    public function number(): int { return 7; }
}

function guardedValue(?GuardedValue $received): int
{
    return $received?->live() === true ? $received->number() : 0;
}

function guardedFallback(?GuardedValue $received): int
{
    return ($received?->live() ?? false) ? $received->number() : 0;
}

final class CacheSlot
{
    private ?GuardedValue $value = null;

    public function number(): int
    {
        $this->value ??= new GuardedValue();
        return $this->value->number();
    }
}

/** @param array<string, ?GuardedValue> $values */
function guardedOffset(array $values, string $key): int
{
    $values[$key] ??= new GuardedValue();
    return $values[$key]->number();
}
