<?php
function foo(int $x) : void {
    $x--;

    if (!is_int($x)) {
        echo "Is a float.";
    } else {
        echo "Is an int.";
    }
}

function bar(int $x) : void {
    $x--;

    if (is_float($x)) {
        echo "Is a float.";
    } else {
        echo "Is an int.";
    }
}
