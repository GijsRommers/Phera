<?php
switch ($n) {
    case 1:
        first();
        // The first case has a side effect.
    case 2:
        second();
}
