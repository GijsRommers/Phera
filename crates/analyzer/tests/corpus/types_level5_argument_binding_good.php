<?php
// Rule: Named arguments and known unpacked entries bind to the corresponding parameter.
function renderArgument(string $label, int $count): string { return $label . $count; }
function checkBindings(): void {
    echo renderArgument(count: 3, label: 'name');
    echo renderArgument(...['name', 3]);
    echo renderArgument(...['count' => 3, 'label' => 'name']);
}
