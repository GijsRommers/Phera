<?php
// Rule: assign.propertyType and property.defaultValue use the declared value type.
class ResultFields {
    public int $count = 'pending';
    public static ?string $label = null;
    /** @var array{state: string} */
    public array $state = ['state' => 'ready'];
}
function storeFields(ResultFields $fields): void {
    $fields->count = 'pending';
    [$fields->count] = ['pending'];
    ResultFields::$label = [1];
    $fields->state = ['state' => 10];
}
