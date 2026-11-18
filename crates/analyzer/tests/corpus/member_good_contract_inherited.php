<?php
// Rule: members / constants / object contexts; member good contract inherited.
class P { function count(): int { return 1; } } class C extends P implements Countable {}
