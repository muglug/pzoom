<?php
$a = false;

foreach (["d", "e", "f"] as $l) {
    foreach (["a", "b", "c"] as $tag) {
        if (!$a) {
            if (rand(0, 10)) {
                $a = true;
                break;
            } else {
                $a = true;
                break;
            }
        }
    }
}
