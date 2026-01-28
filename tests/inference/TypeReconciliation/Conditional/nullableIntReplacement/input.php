<?php
$a = rand(0, 1) ? 5 : null;

$b = (bool)rand(0, 1);

if ($b || $a !== null) {
    $a = 3;
}