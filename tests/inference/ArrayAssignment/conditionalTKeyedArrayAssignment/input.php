<?php
$foo = ["a" => "hello"];
if (rand(0, 10) === 5) {
    $foo["b"] = 1;
}
else {
    $foo["b"] = 2;
}
