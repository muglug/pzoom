<?php
/**
 * @param mixed $arr
 */
function foo($arr) : void {
    if (empty($arr)) {
        return;
    }

    if (isset($arr["a"]) && isset($arr["b"])) {}
}