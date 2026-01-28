<?php
function takesInt(int $i) : void {}

$f = ["a", "b", "c"];
$f[rand(0, 2)] = 5;

$i = rand(0, 2);
if (isset($f[$i]) && !is_string($f[$i])) {
    takesInt($f[$i]);
}