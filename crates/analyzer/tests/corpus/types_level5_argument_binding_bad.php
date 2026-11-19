<?php
// Rule: Named arguments and known unpacked entries bind to the corresponding parameter.
function renderArgument(string $label, int $count): string { return $label . $count; }
function checkBindings(): void {
    echo renderArgument(count: 'wrong', label: 'name');
    echo renderArgument(...['name', 'wrong']);
    echo renderArgument(...['count' => 'wrong', 'label' => 'name']);
}
