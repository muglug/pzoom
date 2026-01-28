<?php
function foo() : void {
    $p = [false, false];
    $i = rand(0, 1);
    if (rand(0, 1) && isset($p[$i])) {
        $p[$i] = true;
    }

    foreach ($p as $q) {
        if ($q) {}
    }
}