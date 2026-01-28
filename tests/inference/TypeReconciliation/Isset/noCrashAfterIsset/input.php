<?php
/**
 * @param string[] $columns
 * @param mixed[]  $options
 */
function foo(array $columns, array $options) : void {
    $arr = $options["b"];

    foreach ($arr as $a) {
        if (isset($columns[$a]["c"])) {
            return;
        }
    }
}