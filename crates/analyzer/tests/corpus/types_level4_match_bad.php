<?php
// Rule: match.unhandled reports a boolean match with a missing arm.
function checkMatch(bool $flag): string {
    return match ($flag) { true => 'yes' };
}
