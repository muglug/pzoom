<?php
function foo(int $x) : void {
    $x = $x + 1;

    if (!is_int($x)) {
        echo "Is a float.";
    } else {
        echo "Is an int.";
    }
}

function bar(int $x) : void {
    $x = $x + 1;

    if (is_float($x)) {
        echo "Is a float.";
    } else {
        echo "Is an int.";
    }
}