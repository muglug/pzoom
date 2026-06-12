<?php
$a = [];

if (rand(0, 1)) {
    $a[] = "hello";
}

if (count($a) > 0) {
    exit;
}
