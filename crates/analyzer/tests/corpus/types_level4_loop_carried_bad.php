<?php
// Rule: Loop conditions and bodies account for writes carried over from earlier iterations.
function checkLoop(int $limit): void {
    for ($position = 0; $position < $limit; ++$position) {
        echo 0 === 1;
    }
}
