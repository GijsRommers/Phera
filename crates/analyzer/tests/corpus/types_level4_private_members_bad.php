<?php
// Rule: Unused private member rules track which constants, properties and methods are used.
final class CheckPrivateMembers {
    private const UNUSED = 1;
    private int $readOnly;
    private int $writeOnly = 0;
    private int $unused;
    private function unusedMethod(): int { return 0; }
    public function read(): int { return $this->readOnly; }
}
