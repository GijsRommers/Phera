<?php
// Rule: Constant comparisons inside a trait are reported when its using classes agree.
trait CheckConsumer {
    public function check(): bool { return 1 === 2; }
}
final class CheckA {
    use CheckConsumer;
}
