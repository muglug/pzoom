<?php
$a = fopen("php://memory", "r");
if (rand(0, 1)) {
    $a = [];
}
$b = (string) $a;
