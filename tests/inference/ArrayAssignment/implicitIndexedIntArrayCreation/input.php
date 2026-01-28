<?php
$foo = [];
$foo[0] = "a";
$foo[1] = "b";
$foo[2] = "c";

$bar = [0, 1, 2];

$bat = [];

foreach ($foo as $i => $text) {
    $bat[$text] = $bar[$i];
}
