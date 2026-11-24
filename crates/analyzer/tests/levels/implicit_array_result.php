<?php

function payload(): array
{
    return ['name' => 'unknown at the call site'];
}

function requireName(string $name): void {}

function readName(): string
{
    $data = payload();
    if (isset($data['name'])) {
        requireName($data['name']);
    }

    return (string) ($data['name'] ?? '');
}

interface LabelSource
{
    /** @return list<string> */
    public function labels(): array;
}

final class FixedLabels implements LabelSource
{
    public function labels(): array
    {
        return ['a label'];
    }
}

function readLabels(FixedLabels $source): void
{
    foreach ($source->labels() as $label) {
        requireName($label);
    }
}
