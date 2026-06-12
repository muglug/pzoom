<?php
foreach (["a", "b", "c"] as $letter) {
    switch ($letter) {
        case "a":
        case "b":
            $foo = 2;
            break;

        default:
            $foo = 3;
            break;
    }

    $moo = $foo;
}
