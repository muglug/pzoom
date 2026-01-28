<?php
/**
 * @param string|array $pv_var
 *
 * @psalm-return ($pv_var is array ? array : string)
 */
function test($pv_var) {
    $return = $pv_var;
    if(!is_array($pv_var)) {
        $return = utf8_encode($pv_var);
    }

    return $return;
}
