<?php
$b = false;

foreach ([1, 2, 3, 4] as $a) {
    if ($a === rand(0, 10)) {
        $b = true;
    }
}
