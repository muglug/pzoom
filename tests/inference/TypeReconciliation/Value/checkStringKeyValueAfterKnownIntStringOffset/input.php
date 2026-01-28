<?php
$foo = [
    "0" => 3,
    "1" => 4,
    "2" => 5,
];

$a = "2";
$foo[$a] = 6;

function takesInt(int $s) : void {}

foreach ($foo as $i => $b) {
    takesInt($i);
}