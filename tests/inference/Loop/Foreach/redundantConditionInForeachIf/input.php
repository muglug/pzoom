<?php
$a = false;

foreach (["a", "b", "c"] as $tag) {
    if (!$a) {
        $a = true;
        break;
    }
}
