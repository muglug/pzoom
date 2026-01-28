<?php
$a = rand(0, 1) ? "a" : "b";

switch ($a) {
    case "a":
        $foo = "hello";
        break;

    case "b":
        $foo = "goodbye";
        break;
}

echo $foo;
