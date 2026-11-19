<?php
// Rule: class narrowing in match and switch arms follows the selected branch.
interface StoredNode {}
class NamedNode implements StoredNode { public function name(): string { return 'name'; } }
function displayNode(StoredNode $node): string {
    switch (true) {
        case $node instanceof NamedNode:
            return $node->name();
        default:
            return '';
    }
}
function displayMatch(StoredNode $node): string {
    return match ($node::class) {
        NamedNode::class => $node->name(),
        default => '',
    };
}
