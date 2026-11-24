<?php
declare(strict_types=1);

class LabelNode
{
    public function name(): string { return 'label'; }
    public function setName(string $name): void {}
}

function guardedArgument(?LabelNode $node): void
{
    $node?->setName($node->name());
    $node->name();
}
