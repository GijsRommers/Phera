<?php

class First { public function identifier(): int { return 1; } }
class Second { public function identifier(): int { return 2; } }

function unionIdentifier(First|Second $source): int
{
    return $source?->identifier();
}

function firstIdentifier(First $source): int
{
    return $source?->identifier() ?? 0;
}
