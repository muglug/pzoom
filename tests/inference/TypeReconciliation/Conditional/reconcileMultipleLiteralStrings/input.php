<?php
/**
 * @param string $param
 * @param "a"|"b"|"c" $param2
 * @return void
 */
function foo($param, $param2) {
    if ( $param === $param2 ) {
        if ($param === "a") {
            echo "x";
        }

        if ($param === "b") {
            echo "y";
        }

        if ($param === "c") {
            echo "z";
        }
    }
}
