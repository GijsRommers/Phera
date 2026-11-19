<?php
foreach ($items as &$item) { $item++; } unset($item); echo 1;
