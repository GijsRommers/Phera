<?php
// Rule: Constant trait conditions must hold for every known using class.
trait CheckConsumer {
    public function check(): bool { return $this instanceof CheckA; }
}
final class CheckA {
    use CheckConsumer;
}
final class CheckB {
    use CheckConsumer;
}
