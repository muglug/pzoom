<?php
/**
 * @param scalar $v
 * @return bool|string
 */
function toString($v)
{
    if (is_numeric($v)) {
        return false;
    }
    return $v;
}
