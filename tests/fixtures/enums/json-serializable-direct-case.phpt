--TEST--
A backed enum uses jsonSerialize when encoded directly
--FILE--
<?php
enum Role: string implements JsonSerializable {
    case Editor = 'edit';
    public function jsonSerialize(): mixed {
        return ['role' => $this->name, 'permission' => $this->value];
    }
}
echo json_encode(Role::Editor), "\n";
--EXPECT--
{"role":"Editor","permission":"edit"}
