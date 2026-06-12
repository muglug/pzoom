<?php
/**
 * @param callable|array $c
 */
function foo($c) : void {
    if (is_array($c) && isset($c[1]) && is_string($c[1])) {
        echo $c[1];
    }
}
