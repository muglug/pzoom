<?php
$b = !!rand(0, 1);

do {
    $s = rand(0, 1);
    if (!$b && $s) {}
} while (!$b && $s);

if ($b) {}
