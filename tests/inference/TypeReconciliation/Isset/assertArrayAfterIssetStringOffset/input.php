<?php
/**
 * @param string|array $a
 */
function _renderInput($a) : array {
    if (isset($a["foo"], $a["bar"])) {
        return $a;
    }

    return [];
}