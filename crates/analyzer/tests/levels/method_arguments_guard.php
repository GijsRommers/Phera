<?php
declare(strict_types=1);

final class Names
{
    public function get(string $key): ?string
    {
        return $key === '' ? null : $key;
    }
}

function takeName(string $name): void {}

function repeatedLookup(Names $names, string $key): void
{
    if ($names->get('title') !== null) {
        takeName($names->get('title'));
        takeName($names->get('subtitle'));
    }
    if ($names->get($key) !== null) {
        takeName($names->get($key));
        $key = '';
        takeName($names->get($key));
    }
}
