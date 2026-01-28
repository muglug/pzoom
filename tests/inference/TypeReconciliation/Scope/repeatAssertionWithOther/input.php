<?php
function getString() : string {
    return "hello";
}
$a = rand(0, 10) ? getString() : null;

if (rand(0, 10) > 1 || is_string($a)) {
    if (is_string($a)) {
        echo strpos($a, "e");
    }
}