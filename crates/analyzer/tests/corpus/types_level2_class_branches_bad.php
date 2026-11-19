<?php
// Rule: only the matching class supplies members inside a match or switch arm.
interface StoredNode {}
class NamedNode implements StoredNode { public function name(): string { return 'name'; } }
class NumberedNode implements StoredNode { public function number(): int { return 1; } }
function displayNode(StoredNode $node): string {
    switch (true) {
        case $node instanceof NumberedNode:
            return $node->name();
        default:
            return '';
    }
}
function displayMatch(StoredNode $node): string {
    return match ($node::class) {
        NumberedNode::class => $node->name(),
        default => '',
    };
}
