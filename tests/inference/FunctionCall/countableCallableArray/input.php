<?php
/** @param callable|false $x */
function example($x) : void {
    if (is_array($x)) {
        echo "Count is: " . count($x);
    }
}
