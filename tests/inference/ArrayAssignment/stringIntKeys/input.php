<?php
/**
 * @param array<15|"17"|"hello", string> $arg
 * @return bool
 */
function foo($arg) {
    foreach ($arg as $k => $v) {
        if ( $k === 15 ) {
            return true;
        }

        if ( $k === 17 ) {
            return false;
        }
    }

    return true;
}

$x = ["15" => "a", 17 => "b"];
foo($x);
