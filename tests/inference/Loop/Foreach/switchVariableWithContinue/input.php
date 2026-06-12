<?php
foreach (["a", "b", "c"] as $letter) {
    switch ($letter) {
        case "b":
            $foo = 1;
            break;
        case "c":
            $foo = 2;
            break;
        default:
            continue 2;
    }

    $moo = $foo;
}
