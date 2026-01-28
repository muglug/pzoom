<?php
function takesString(string $i) : void {}

$f = [0, 1, 2];
$f[rand(0, 2)] = "hello";

$i = rand(0, 2);
if (isset($f[$i]) && !is_int($f[$i])) {
    takesString($f[$i]);
}