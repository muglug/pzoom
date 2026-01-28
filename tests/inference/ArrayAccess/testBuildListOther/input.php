<?php
$list = [];
$entropy = random_int(0, 2);
if ($entropy === 0) {
    $list[] = "A";
} elseif ($entropy === 1) {
    $list[] = "B";
}

$list[] = "C";
