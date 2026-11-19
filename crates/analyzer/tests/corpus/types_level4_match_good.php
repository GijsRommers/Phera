<?php
// Rule: match.unhandled accepts all of the boolean cases without a default.
function checkMatch(bool $flag): string {
    return match ($flag) { true => 'yes', false => 'no' };
}
