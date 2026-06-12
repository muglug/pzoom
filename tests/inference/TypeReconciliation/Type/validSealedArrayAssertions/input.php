<?php
/** @var array{a: string, b: string, c?: string} */
$a = [];

if (count($a) > 2) {
    echo "Have C!";
}

if (count($a) < 3) {
    echo "Do not have C!";
}
