<?php
/** @param list<int> $list */
function loops(array $list, bool $flag): void
{
    for ($counter = 0; $flag; $counter++) { break; }
    foreach ($list as $counter) { echo $counter; }

    foreach ([1] as $fromLiteral) { echo $fromLiteral; }
    foreach ($list as $fromLiteral) { echo $fromLiteral; }

    foreach ($list as $fromList) { echo $fromList; }
    foreach ($list as $fromList) { echo $fromList; }

    if ($flag) {
        $fromBranch = 1;
    }
    foreach ($list as $fromBranch) { echo $fromBranch; }
}
