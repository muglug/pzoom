<?php
function foo() : void {
    switch(rand() % 4) {
        case 0:
            echo "here";
            break;
        case 1:
            $x = rand() % 4;
        case 2:
            if (isset($x) && $x > 2) {
                echo "$x is large";
            }
            break;
    }
}
