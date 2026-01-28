<?php
$a = [];

if (rand(0, 1)) {
    $a[] = 4;
}

if ($a) {
} elseif (rand(0, 1)) {
    $a = null;
}
