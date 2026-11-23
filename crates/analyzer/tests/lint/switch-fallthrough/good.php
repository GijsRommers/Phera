<?php
switch ($n) {
    case 1:
        first();
        // no break
    case 2:
        second();
        // Fall through
    case 3:
        if ($success) { return; } else { throw new \Exception(); }
    default:
        break;
}
