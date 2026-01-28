<?php
function foo(int $x) : void {
    if (rand(0, 1)) {
        $x = 125;
    }

    if (is_float($x)) {
        echo "Is a float.";
    } else {
        echo "Is an int.";
    }
}
