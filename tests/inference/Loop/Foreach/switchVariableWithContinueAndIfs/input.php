<?php
foreach (["a", "b", "c"] as $letter) {
    switch ($letter) {
        case "a":
            if (rand(0, 10) === 1) {
                continue 2;
            }
            $foo = 1;
            break;
        case "b":
            if (rand(0, 10) === 1) {
                continue 2;
            }
            $foo = 2;
            break;
        default:
            continue 2;
    }

    $moo = $foo;
}
