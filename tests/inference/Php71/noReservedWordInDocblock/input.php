<?php
/**
 * @param Closure():(resource|false) $op
 * @return resource|false
 */
function create_resource($op) {
    return $op();
}
