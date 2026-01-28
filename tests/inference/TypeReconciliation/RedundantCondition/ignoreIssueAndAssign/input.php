<?php
function foo(): stdClass {
    return new stdClass;
}

$b = null;

foreach ([0, 1] as $i) {
    $a = foo();

    if (!empty($a)) {
        $b = $a;
    }
}