<?php

/**
 * @param array{x?: int, y?: int, z?: int} $a
 * @param 'x'|'y'|'z' $b
 * @return void
 */
function foo( $a, $b ) {
    if ( !isset( $a[ $b ] ) ) {
        return;
    }

    echo $a[ $b ];
}
