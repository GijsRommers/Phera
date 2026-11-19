<?php
// Rule: Unused private member rules track which constants, properties and methods are used.
final class CheckPrivateMembers {
    private const USED = 1;
    private int $value;
    private function read(): int { return $this->value; }
    public function run(): int {
        $this->value = self::USED;
        return $this->read();
    }
}
