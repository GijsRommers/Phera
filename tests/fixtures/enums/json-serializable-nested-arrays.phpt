--TEST--
A backed enum uses jsonSerialize at more than one array depth
--FILE--
<?php
enum Role: string implements JsonSerializable {
    case Editor = 'edit';
    public function jsonSerialize(): mixed {
        return ['role' => $this->name, 'permission' => $this->value];
    }
}
echo json_encode([Role::Editor, ["nested" => [Role::Editor]]]), "\n";
--EXPECT--
[{"role":"Editor","permission":"edit"},{"nested":[{"role":"Editor","permission":"edit"}]}]
