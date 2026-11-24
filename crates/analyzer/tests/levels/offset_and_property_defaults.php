<?php

final class OptionalCount
{
    private ?int $count = null;

    public function count(): ?int
    {
        return $this->count;
    }
}

interface LookupContract
{
    /** @return int|null */
    public function find();
}

final class Lookup implements LookupContract
{
    public function find()
    {
        $unused = 1;
    }
}

/** @param list<string> $values */
function offsetByFloat(array $values, float $key): mixed
{
    return $values[$key];
}
