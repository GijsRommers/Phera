<?php
declare(strict_types=1);

final class TaggedNode
{
    public function __construct(public string $tag) {}
    public function kind(): string { return $this->tag; }
}

function tagIfMatching(?TaggedNode $node, string $kind): ?string
{
    return $node?->kind() === $kind ? $node->tag : null;
}
