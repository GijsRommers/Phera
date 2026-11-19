<?php
// Rule: a declaration controls assignment to instance and static typed properties.
class ResultFields {
    public int $count = 0;
    public static ?string $label = null;
    /** @var array{state: string} */
    public array $state = ['state' => 'ready'];
}
function storeFields(ResultFields $fields): void {
    $fields->count += 3;
    [$fields->count] = [8];
    ResultFields::$label = 'updated';
    $fields->state = ['state' => 'finished'];
}
