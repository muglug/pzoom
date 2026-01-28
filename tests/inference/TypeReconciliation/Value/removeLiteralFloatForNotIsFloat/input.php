<?php
function takesString(string $i) : void {}

$f = [1.1, 1.2, 1.3];
$f[rand(0, 2)] = "hello";

$i = rand(0, 2);
if (isset($f[$i]) && !is_float($f[$i])) {
    takesString($f[$i]);
}